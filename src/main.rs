//! A Rust port of sbt-extras.
//! Author: Dale Wijnand <dale.wijnand@gmail.com>

#![allow(dead_code)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
//#![allow(unused_assignments)]
#![allow(unused_imports)]
//#![allow(unused_variables)]

use std::env::*;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fs;
use std::fs::File;
use std::io::{ BufReader, BufWriter, };
use std::io::prelude::*;
use std::os::unix::process::CommandExt;
use std::path::{ Path, PathBuf, };
use std::process::{ Command, exit, };

const sbt_release_version: &'static str = "0.13.16";

const buildProps: &'static str = "project/build.properties";

const sbt_launch_ivy_release_repo: &'static str = "http://repo.typesafe.com/typesafe/ivy-releases";
const sbt_launch_mvn_release_repo: &'static str = "http://repo.scala-sbt.org/scalasbt/maven-releases";

const default_jvm_opts_common: [&'static str; 3] = ["-Xms512m", "-Xmx1536m", "-Xss2m"];

#[macro_use] extern crate lazy_static;

extern crate sha1;

lazy_static! {
    static ref HOME: PathBuf = home_dir().unwrap();
    static ref script_name: String = current_exe().unwrap().file_name().unwrap().to_string_lossy().into_owned();
}

macro_rules! echoerr(($($arg:tt)*) => (writeln!(&mut ::std::io::stderr(), $($arg)*).unwrap();));
macro_rules!     die(($($arg:tt)*) => (println!("Aborting {}", format!($($arg)*)); ::std::process::exit(1);));

fn build_props_sbt() -> String {
    if let Ok(f) = File::open(buildProps) {
        let f = BufReader::new(f);
        for line in f.lines() {
            let line = line.unwrap();
            if line.starts_with("sbt.version") {
                let line = line.replace("=", " ");
                let line = line.replace("\r", " ");
                return line.split_whitespace().nth(1).unwrap().to_owned();
            }
        }
    }
    "".to_owned()
}

fn url_base(version: &str) -> &'static str { match version {
    s if s.starts_with("0.7.")  => "http://simple-build-tool.googlecode.com",
    s if s.starts_with("0.10.") => sbt_launch_ivy_release_repo,
    "0.11.1" | "0.11.2"         => sbt_launch_ivy_release_repo,
 // "0.*-yyyymmdd-hhMMss"       => sbt_launch_ivy_snapshot_repo, // https://repo.scala-sbt.org/scalasbt/ivy-snapshots
    s if s.starts_with("0.")    => sbt_launch_ivy_release_repo,
 // "*-yyyymmdd-hhMMss"         => sbt_launch_mvn_snapshot_repo, // http://repo.scala-sbt.org/scalasbt/maven-snapshots
    _                           => sbt_launch_mvn_release_repo,
} }

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

fn download_url(sbt_version: &str, url: &str, jar: &Path) -> bool {
    echoerr!("Downloading sbt launcher for {}:", sbt_version);
    echoerr!("  From  {}", url);
    echoerr!("    To  {}", jar.display());

    fs::create_dir_all(jar.parent().unwrap()).unwrap();

    extern crate curl;
    let mut jar2 = BufWriter::new(File::create(jar).unwrap());
    let mut easy = curl::easy::Easy::new();
    easy.follow_location(true).unwrap();
    easy.url(url).unwrap();
    easy.write_function(move |data| Ok(jar2.write(data).unwrap())).unwrap();
    easy.perform().unwrap();
    File::open(jar).is_ok()
}

struct App {
               sbt_jar: PathBuf,
           sbt_version: String,
  sbt_explicit_version: String,
               verbose: bool,
              java_cmd: String,
        sbt_launch_dir: PathBuf,
        extra_jvm_opts: Vec<String>,   // args to jvm via files or environment variables
             java_args: Vec<String>,   // pull -J and -D options to give to java
          sbt_commands: Vec<String>,
         residual_args: Vec<String>,
               sbt_new: bool,
}

impl App {
    fn new() -> App {
        App {
                 sbt_jar: PathBuf::new(),
             sbt_version: Default::default(),
    sbt_explicit_version: Default::default(),
                 verbose: Default::default(),
                java_cmd: "java".into(),
          sbt_launch_dir: { let mut p = PathBuf::from(&*HOME); p.push(".sbt/launchers"); p },
          extra_jvm_opts: Default::default(),
               java_args: Default::default(),
            sbt_commands: Default::default(),
           residual_args: Default::default(),
                 sbt_new: Default::default(),
        }
    }

    // TODO: See if this can become a macro
    fn vlog(&self, s: &str) -> bool { if self.verbose { echoerr!("{}", s) }; self.verbose }

    fn set_sbt_version(&mut self) {
        if self.sbt_explicit_version.is_empty() {
            self.sbt_version=build_props_sbt()
        } else {
            self.sbt_version=self.sbt_explicit_version.to_owned()
        }
        if self.sbt_version.is_empty() { self.sbt_version=sbt_release_version.to_owned() }
    }

    fn addJava(&mut self, s: &str) {
        self.vlog(&format!("[addJava] arg = '{}'", s));
        self.java_args.push(s.into());
    }

    fn addResidual(&mut self, s: &str) {
        self.vlog(&format!("[residual] arg = '{}'", s));
        self.residual_args.push(s.into());
    }

    fn addDebugger(&mut self, port: u16) {
        self.addJava("-Xdebug");
        self.addJava(&format!("-Xrunjdwp:transport=dt_socket,server=y,suspend=n,address={}", port));
    }

    // MaxPermSize critical on pre-8 JVMs but incurs noisy warning on 8+
    fn default_jvm_opts(&self) -> Vec<String> {
        // TODO: Don't add MaxPermSize if on Java 8+
        let mut opts: Vec<&'static str> = Vec::with_capacity(default_jvm_opts_common.len() + 1);
        opts.push("-XX:MaxPermSize=384m");
        opts.extend_from_slice(&default_jvm_opts_common);
        opts.iter().map(|x| x.to_string()).collect()
    }

    fn execRunner<S: AsRef<OsStr>>(&self, args: &[S]) {
        self.vlog("# Executing command line:") && {
            for arg in args {
                let arg = arg.as_ref();
                if !arg.is_empty() {
                    let arg = arg.to_string_lossy();
                    if arg.contains(" ") {
                        echoerr!("\"{}\"", arg);
                    } else {
                        echoerr!("{}", arg);
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

    fn jar_file(&self, version: &str) -> PathBuf {
        let mut p = PathBuf::from(&self.sbt_launch_dir);
        p.push(version);
        p.push("sbt-launch.jar");
        p
    }

    fn acquire_sbt_jar(&mut self) -> bool {
        ({
            self.sbt_jar = self.jar_file(&self.sbt_version);
            File::open(self.sbt_jar.as_path()).is_ok()
        }) || ({
            self.sbt_jar = PathBuf::from(&*HOME);
            self.sbt_jar.push(format!(".ivy2/local/org.scala-sbt/sbt-launch/{}/jars/sbt-launch.jar", self.sbt_version));
            File::open(self.sbt_jar.as_path()).is_ok()
        }) || ({
            self.sbt_jar = self.jar_file(&self.sbt_version);
            download_url(&self.sbt_version, &make_url(&self.sbt_version), &self.sbt_jar)
        })
    }

    fn usage(&mut self) {
        self.set_sbt_version();
        print!("\
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
  # The default set is used if JVM_OPTS is unset and no -jvm-opts file is found
  <default>        {default_jvm_opts}
  -Dkey=val        pass -Dkey=val directly to the jvm
  -J-X             pass option -X directly to the jvm (-J is stripped)
",
    script_name=*script_name,
    default_jvm_opts=self.default_jvm_opts().join(" "),
);
    }

    fn process_args(&mut self) {
        fn require_arg(tpe: &str, opt: &str, arg: &str) {
            if arg.is_empty() || &arg[0..1] == "-" {
                die!("{} requires <{}> argument", opt, tpe);
            }
        }
        let mut args = args().skip(1); // skip the path of the executable
        while let Some(arg) = args.next() {
            let mut next = || -> String { args.next().unwrap_or("".into()) };
            let arg = arg.as_ref();
            match arg {
                "-h" | "-help"           => { self.usage(); exit(1) },
                "-v"                     => self.verbose = true,
                "-jvm-debug"             => { let next = next(); require_arg("port", arg, &next); self.addDebugger(next.parse().unwrap()) },
                "-sbt-jar"               => { let next = next(); require_arg("path", arg, &next); self.sbt_jar = PathBuf::from(next) },
                s if s.starts_with("-D") => self.addJava(s),
                s if s.starts_with("-J") => self.addJava(&s[2..]),
                "new"                    => { self.sbt_new=true; self.sbt_explicit_version=sbt_release_version.to_owned(); self.addResidual(arg) },
                s                        => self.addResidual(s),
            }
        }
    }

    fn run(&mut self) {
        let argumentCount = self.residual_args.len();

        self.set_sbt_version();
        self.vlog(&format!("Detected sbt version {}", self.sbt_version));

        if argumentCount == 0 {
            self.vlog(&format!("Starting {}: invoke with -help for other options", *script_name));
            self.residual_args = vec!["shell".into()];
        }

        // verify this is an sbt dir
        if !File::open(PathBuf::from("build.sbt")).is_ok() && !PathBuf::from("project").is_dir() && !self.sbt_new {
            print!("\
{pwd} doesn't appear to be an sbt project.
", pwd=current_dir().unwrap().display());
            exit(1);
        }

        // no jar? download it.
        File::open(self.sbt_jar.as_path()).is_ok() || self.acquire_sbt_jar() || {
            // still no jar? uh-oh.
            println!("Download failed. Obtain the jar manually and place it at {}", self.sbt_jar.display());
            exit(1);
        };

        self.vlog("Using default jvm options");
        self.extra_jvm_opts=self.default_jvm_opts();

        let mut exec_args: Vec<&OsStr> = Vec::new();
        exec_args.push(self.java_cmd.as_ref());
        exec_args.append(&mut self.extra_jvm_opts.iter().map(AsRef::as_ref).collect());
        exec_args.append(&mut self.java_args.iter().map(AsRef::as_ref).collect());
        exec_args.append(&mut vec!["-jar".as_ref(), self.sbt_jar.as_ref()]);
        exec_args.append(&mut self.sbt_commands.iter().map(AsRef::as_ref).collect());
        exec_args.append(&mut self.residual_args.iter().map(AsRef::as_ref).collect());

        self.execRunner(&exec_args)
    }
}

fn main() {
    // let mut app = App::new();
    // app.process_args();
    // app.run()
    let baseDirPath = current_dir().unwrap();
    let portFilePath = { let mut p = baseDirPath; p.push("project/target/active.json"); p };
    println!("{}", portFilePath.display());

    // TODO: Use a less naive path to URI conversion
    let portFileUri = format!("file://{}", portFilePath.display());
    println!("{}", portFileUri);

    // TODO: Figure out how to use sha1's opt-in hexdigest() method
    let sha1 = sha1::Sha1::from(portFileUri);
    use std::string::ToString;
    let hash = sha1.digest().to_string();
    println!("{}", hash);

    // TODO: Add if hash.len() > 3 condition
    let halfHash = &hash[0..hash.len() / 2];
    println!("{}", halfHash);

    let homeDir = home_dir().unwrap();
    let socketFile = { let mut p = homeDir; p.push(".sbt/1.0/server"); p.push(halfHash); p.push("sock"); p };
    println!("{}", socketFile.display());
}
