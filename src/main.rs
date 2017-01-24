#![allow(non_upper_case_globals)]

#![allow(dead_code)]
#![allow(unused_assignments)]
#![allow(unused_variables)]

#[macro_use] extern crate lazy_static;

use std::ffi::OsStr;
use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;
use std::path::{ Path, PathBuf };

lazy_static! {
    static ref HOME: PathBuf = std::env::home_dir().unwrap();

    static ref build_props: PathBuf = PathBuf::from("project/build.properties");

    static ref sbt_launch_dir: PathBuf = { let mut p = PathBuf::from(&*HOME); p.push(".sbt/launchers"); p };

    static ref script_name: String = {
        let n = std::env::args().nth(0).unwrap();
        let n = Path::new(&n).file_name().unwrap().to_str().unwrap();
        n.to_string()
    };

}

const sbt_launch_ivy_release_repo: &'static str = "http://repo.typesafe.com/typesafe/ivy-releases";
const sbt_launch_mvn_release_repo: &'static str = "http://repo.scala-sbt.org/scalasbt/maven-releases";

macro_rules! echoerr(($($arg:tt)*) => (writeln!(&mut ::std::io::stderr(), $($arg)*).unwrap();));

fn die(s: &str) { println!("Aborting: {}", s); std::process::exit(1) }

fn build_props_sbt() -> String {
    if let Ok(f) = File::open(&*build_props) {
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

fn jar_file(version: &str) -> PathBuf {
    let mut p = PathBuf::from(&*sbt_launch_dir);
    p.push(version);
    p.push("sbt-launch.jar");
    p
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

    std::fs::create_dir_all(jar.parent().unwrap()).unwrap();

    extern crate hyper;
    let mut r = BufReader::new(hyper::client::Client::new().get(url).send().unwrap());
    let mut buf = [0; 16384];
    let mut jar2 = std::io::BufWriter::new(File::create(jar).unwrap());
    while {
        let bc = r.read(&mut buf).unwrap();
        jar2.write(&buf[0..bc]).unwrap();
        bc > 0
    } {}
    jar2.flush().unwrap();
    File::open(jar).is_ok()
}

struct App {
           sbt_jar: PathBuf,
       sbt_version: String,
           verbose: bool,
          java_cmd: String,
    extra_jvm_opts: Vec<String>,
         java_args: Vec<String>,
      sbt_commands: Vec<String>,
     residual_args: Vec<String>,
}

impl App {
    fn new() -> App {
        App {
                 sbt_jar: PathBuf::new(),
             sbt_version: Default::default(),
                 verbose: Default::default(),
                java_cmd: "java".into(),
          extra_jvm_opts: vec!["-Xms512m".into(), "-Xmx1536m".into(), "-Xss2m".into()],
               java_args: Default::default(),
            sbt_commands: Default::default(),
           residual_args: Default::default(),
        }
    }

    fn vlog(&self, s: &str) -> bool { if self.verbose { echoerr!("{}", s); true } else { false } }

    fn set_sbt_version(&mut self) {
        self.sbt_version=build_props_sbt();
        // sbt_version="${sbt_explicit_version:-$(build_props_sbt)}"
        // [[ -n "$sbt_version" ]] || sbt_version=$sbt_release_version
    }

    fn add_java(&mut self, s: &str) {
        self.vlog(&format!("[add_java] arg = {}", s));
        self.java_args.push(s.into());
    }

    fn add_residual(&mut self, s: &str) {
        self.vlog(&format!("[residual] arg = {}", s));
        self.residual_args.push(s.into());
    }

    fn add_debugger(&mut self, port: u16) {
        self.add_java("-Xdebug");
        self.add_java(&format!("-Xrunjdwp:transport=dt_socket,server=y,suspend=n,address={}", port));
    }

    fn process_args(&mut self) {
        fn require_arg(tpe: &str, opt: &str, arg: &str) {
            if arg.is_empty() || &arg[0..1] == "-" {
                die(&format!("{} requires <{}> argument", opt, tpe));
            }
        }
        let args = std::env::args();
        let mut args = args.skip(1); // skip the path of the executable
        while let Some(arg) = args.next() {
            let mut next = || -> String { args.next().unwrap_or("".into()) };
            let arg = arg.as_ref();
            match arg {
                "-v"         => self.verbose = true,
                "-jvm-debug" => { let next = next(); require_arg("port", arg, &next); self.add_debugger(next.parse().unwrap()) },
                s            => panic!("fu"), // self.add_residual(&s),
            }
        }
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

    fn exec_runner<S: AsRef<OsStr>>(&self, args: &[S]) {
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

        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&args[0]).args(&args[1..]).exec();
        println!("error: {}", err);
        if let Some(err) = err.raw_os_error() {
            std::process::exit(err);
        }
        std::process::exit(-1)
    }

    fn run(&mut self) {
        let argument_count = self.residual_args.len();

        self.set_sbt_version();

        if argument_count == 0 {
            self.vlog(&format!("Starting {}: invoke with -help for other options", *script_name));
            self.residual_args = vec!["shell".into()];
        }

        // no jar? download it.
        File::open(self.sbt_jar.as_path()).is_ok() || self.acquire_sbt_jar() || {
            // still no jar? uh-oh.
            println!("Download failed. Obtain the jar manually and place it at {}", self.sbt_jar.display());
            std::process::exit(1);
        };

        let mut exec_args: Vec<&OsStr> = Vec::new();
        exec_args.push(self.java_cmd.as_ref());
        exec_args.append(&mut self.extra_jvm_opts.iter().map(AsRef::as_ref).collect());
        exec_args.append(&mut self.java_args.iter().map(AsRef::as_ref).collect());
        exec_args.append(&mut vec!["-jar".as_ref(), self.sbt_jar.as_ref()]);
        exec_args.append(&mut self.sbt_commands.iter().map(AsRef::as_ref).collect());
        exec_args.append(&mut self.residual_args.iter().map(AsRef::as_ref).collect());

        self.exec_runner(&exec_args)
    }
}

fn main() {
    let mut app = App::new();
    app.process_args();
    app.run()
}
