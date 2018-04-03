#![allow(dead_code)]
#![allow(non_upper_case_globals)]
//#![allow(unused_assignments)]
#![allow(unused_imports)]
//#![allow(unused_variables)]

const sbt_release_version: &str = "0.13.16";

use std::env;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fs;
use std::fs::File;
use std::io::{ BufReader, BufWriter, };
use std::io::prelude::*;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{ Path, PathBuf, };
use std::process::{ Command, exit, };

use curl;
use void::Void;

lazy_static! {
    static ref HOME: PathBuf = {
        env::home_dir().expect("failed to get the path of the current user's home directory")
    };
    static ref WD: PathBuf = {
        env::current_dir().expect("failed to get the current working directory")
    };
    static ref script_name: String = {
        let current_exe = env::current_exe().expect("failed to get the full filesystem path of the current running executable");
        current_exe.file_name().expect("current_exe's file_name should not be '..'").to_string_lossy().into_owned()
    };
    static ref ARGS: Vec<String> = env::args().collect();
    static ref sbt_launch_dir: PathBuf = PathBuf::from(&*HOME).join(".sbt/launchers");
}

macro_rules! die(($($arg:tt)*) => (println!("Aborting {}", format!($($arg)*)); ::std::process::exit(1);));

fn build_props_sbt() -> String {
    File::open("project/build.properties")
        .ok()
        .and_then(|f|
            BufReader::new(f)
                .lines()
                .map(|l| l.expect("reading lines from build properties wouldn't fail"))
                .find(|l| l.starts_with("sbt.version"))
                .map(|l| l.split('=').nth(1).expect("an sbt version on the right of sbt.version=").trim().to_owned())
        )
        .unwrap_or_else(|| "".to_owned())
}

fn url_base(version: &str) -> &'static str {
    let ivy_releases_url = "http://repo.typesafe.com/typesafe/ivy-releases";
    let mvn_releases_url = "http://repo.scala-sbt.org/scalasbt/maven-releases";
  //let ivy_snapshot_url = "http://repo.scala-sbt.org/scalasbt/ivy-snapshots";
  //let mvn_snapshot_url = "http://repo.scala-sbt.org/scalasbt/maven-snapshots";
    match version {
        s if s.starts_with("0.7.")  => "http://simple-build-tool.googlecode.com",
        s if s.starts_with("0.10.") => ivy_releases_url,
        "0.11.1" | "0.11.2"         => ivy_releases_url,
    //  "0.*-yyyymmdd-hhMMss"       => ivy_snapshot_url,
        s if s.starts_with("0.")    => ivy_releases_url,
    //  "*-yyyymmdd-hhMMss"         => mvn_snapshot_url,
        _                           => mvn_releases_url,
    }
}

fn make_url(version: &str) -> String {
    let base = url_base(version);
    match version {
        s if s.starts_with("0.7.")  => format!("{}/files/sbt-launch-0.7.7.jar", base),
        s if s.starts_with("0.10.") => format!("{}/org.scala-tools.sbt/sbt-launch/{}/sbt-launch.jar", base, version),
        "0.11.1" | "0.11.2"         => format!("{}/org.scala-tools.sbt/sbt-launch/{}/sbt-launch.jar", base, version),
        s if s.starts_with("0.")    => format!("{}/org.scala-sbt/sbt-launch/{}/sbt-launch.jar", base, version),
        _                           => format!("{}/org/scala-sbt/sbt-launch/{}/sbt-launch.jar", base, version),
    }
}

fn jar_file(version: &str) -> PathBuf {
    PathBuf::from(&*sbt_launch_dir).join(version).join("sbt-launch.jar")
}

fn download_url(sbt_version: &str, url: &str, jar: &Path) -> bool {
    eprintln!("Downloading sbt launcher for {}:", sbt_version);
    eprintln!("  From  {}", url);
    eprintln!("    To  {}", jar.display());

    fs::create_dir_all(jar.parent().unwrap()).unwrap();

    let mut jar2 = BufWriter::new(File::create(jar).unwrap());
    let mut easy = curl::easy::Easy::new();
    easy.follow_location(true).unwrap();
    easy.url(url).unwrap();
    easy.write_function(move |data| Ok(jar2.write(data).unwrap())).unwrap();
    easy.perform().unwrap();
    File::open(jar).is_ok()
}

#[derive(Default)]
pub struct Launcher {
             sbt_version: String,
    sbt_explicit_version: String,
                 verbose: bool,
                java_cmd: String,
                jvm_opts: Vec<String>,   // pull -J and -D options to give to java
                 sbt_jar: PathBuf,
                 sbt_new: bool,
           residual_args: Vec<String>,
}

impl Launcher {
    pub fn new() -> Self {
        Self {
            java_cmd: "java".into(),
            ..Default::default()
        }
    }

    // TODO: See if this can become a macro
    fn vlog(&self, s: &str) { if self.verbose { eprintln!("{}", s) } }

    fn set_sbt_version(&mut self) {
        if self.sbt_explicit_version.is_empty() {
            self.sbt_version=build_props_sbt()
        } else {
            self.sbt_version=self.sbt_explicit_version.to_owned()
        }
        if self.sbt_version.is_empty() { self.sbt_version=sbt_release_version.to_owned() }
    }

    fn add_jvm_opt(&mut self, s: &str) {
        self.vlog(&format!("[java] arg = '{}'", s));
        self.jvm_opts.push(s.into());
    }

    fn add_residual(&mut self, s: &str) {
        self.vlog(&format!("[residual] arg = '{}'", s));
        self.residual_args.push(s.into());
    }

    fn add_debugger(&mut self, port: u16) {
        self.add_jvm_opt("-Xdebug");
        self.add_jvm_opt(&format!("-Xrunjdwp:transport=dt_socket,server=y,suspend=n,address={}", port));
    }

    // MaxPermSize critical on pre-8 JVMs but incurs noisy warning on 8+
    fn default_jvm_opts(&self) -> Vec<String> {
        // TODO: Don't add MaxPermSize if on Java 8+
        let opts = ["-XX:MaxPermSize=384m", "-Xms512m", "-Xmx1536m", "-Xss2m"];
        opts.iter().map(|s| s.to_string()).collect()
    }

    fn exec_runner<S: AsRef<OsStr>>(&self, args: &[S]) {
        self.vlog("# Executing command line:");
        if self.verbose {
            for arg in args {
                let arg = arg.as_ref();
                if !arg.is_empty() {
                    let arg = arg.to_string_lossy();
                    if arg.contains(' ') {
                        eprintln!("\"{}\"", arg)
                    } else {
                        eprintln!("{}", arg)
                    }
                }
            }
            self.vlog("")
        };

        let err = Command::new(&args[0]).args(&args[1..]).exec();
        println!("error: {}", err);
        if let Some(err) = err.raw_os_error() {
            exit(err);
        }
        exit(-1)
    }

    fn acquire_sbt_jar(&mut self) -> bool {
        ({
            self.sbt_jar = jar_file(&self.sbt_version);
            File::open(self.sbt_jar.as_path()).is_ok()
        }) || ({
            self.sbt_jar = PathBuf::from(&*HOME);
            self.sbt_jar.push(format!(".ivy2/local/org.scala-sbt/sbt-launch/{}/jars/sbt-launch.jar", self.sbt_version));
            File::open(self.sbt_jar.as_path()).is_ok()
        }) || ({
            self.sbt_jar = jar_file(&self.sbt_version);
            download_url(&self.sbt_version, &make_url(&self.sbt_version), &self.sbt_jar)
        })
    }

    fn usage(&mut self) {
        self.set_sbt_version();
        println!("\
Usage: {script_name} [options]

Note that options which are passed along to sbt begin with -- whereas
options to this runner use a single dash. Any sbt command can be scheduled
to run first by prefixing the command with --, so --warn, --error and so on
are not special.

  -h | -help         print this message
  -v                 verbose operation (this runner is chattier)
  -jvm-debug <port>  turn on JVM debugging, open at the given port.
  -sbt-jar <path>    use the specified jar as the sbt launcher

  # passing options to the jvm - note it does NOT use JAVA_OPTS due to pollution
  <default>        {default_jvm_opts}
  -Dkey=val        pass -Dkey=val directly to the jvm
  -J-X             pass option -X directly to the jvm (-J is stripped)",
            script_name=*script_name,
            default_jvm_opts=self.default_jvm_opts().join(" "),
        )
    }

    pub fn run(&mut self) {
        let mut args = ARGS.iter().skip(1); // skip the path of the executable
        while let Some(arg) = args.next() {
            let blank = &String::new();
            let mut require_arg = |tpe| {
                let opt = arg;
                let arg = args.next().unwrap_or(blank);
                if arg.is_empty() || &arg[0..1] == "-" {
                    die!("{opt} requires <{type}> argument", opt=opt, type=tpe);
                }
                arg
            };
            match arg.as_ref() {
                "-h" | "-help"           => { self.usage(); exit(1) },
                "-v"                     => self.verbose = true,
                "-jvm-debug"             => { let arg = require_arg("port"); self.add_debugger(arg.parse().unwrap()) },
                "-sbt-jar"               => { let arg = require_arg("path"); self.sbt_jar = PathBuf::from(arg) },
                s if s.starts_with("-D") => self.add_jvm_opt(s),
                s if s.starts_with("-J") => self.add_jvm_opt(&s[2..]),
                "new"                    => { self.sbt_new=true; self.sbt_explicit_version=sbt_release_version.to_owned(); self.add_residual(arg) },
                s                        => self.add_residual(s),
            }
        }

        let args_count = self.residual_args.len();

        self.set_sbt_version();
        self.vlog(&format!("Detected sbt version {}", self.sbt_version));

        if args_count == 0 {
            self.vlog(&format!("Starting {}: invoke with -help for other options", *script_name));
            self.residual_args = vec!["shell".into()];
        }

        // verify this is an sbt dir
        if !File::open(PathBuf::from("build.sbt")).is_ok() && !PathBuf::from("project").is_dir() && !self.sbt_new {
            println!("{pwd} doesn't appear to be an sbt project.", pwd=WD.display());
            exit(1);
        }

        // no jar? download it.
        File::open(self.sbt_jar.as_path()).is_ok() || self.acquire_sbt_jar() || {
            // still no jar? uh-oh.
            println!("Download failed. Obtain the jar manually and place it at {}", self.sbt_jar.display());
            exit(1);
        };

        self.vlog("Using default jvm options");
        let default_jvm_opts=self.default_jvm_opts();

        let mut exec_args: Vec<&OsStr> = Vec::new();
        exec_args.push(self.java_cmd.as_ref());
        exec_args.append(&mut default_jvm_opts.iter().map(AsRef::as_ref).collect());
        exec_args.append(&mut self.jvm_opts.iter().map(AsRef::as_ref).collect());
        exec_args.append(&mut vec!["-jar".as_ref(), self.sbt_jar.as_ref()]);
        exec_args.append(&mut self.residual_args.iter().map(AsRef::as_ref).collect());

        self.exec_runner(&exec_args)
    }
}
