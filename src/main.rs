//! A Rust port of sbt-extras.
//! Author: Dale Wijnand <dale.wijnand@gmail.com>

#![allow(dead_code)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
//#![allow(unused_assignments)]
#![allow(unused_imports)]
//#![allow(unused_variables)]

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

const sbt_release_version: &'static str = "0.13.16";

const buildProps: &'static str = "project/build.properties";

const sbt_launch_ivy_release_repo: &'static str = "http://repo.typesafe.com/typesafe/ivy-releases";
const sbt_launch_mvn_release_repo: &'static str = "http://repo.scala-sbt.org/scalasbt/maven-releases";

const default_jvm_opts_common: [&'static str; 3] = ["-Xms512m", "-Xmx1536m", "-Xss2m"];

#[macro_use] extern crate serde_json;
use serde_json::Value;

extern crate jsonrpc_lite;
use jsonrpc_lite::JsonRpc;

macro_rules! die(($($arg:tt)*) => (println!("Aborting {}", format!($($arg)*)); ::std::process::exit(1);));

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
    eprintln!("Downloading sbt launcher for {}:", sbt_version);
    eprintln!("  From  {}", url);
    eprintln!("    To  {}", jar.display());

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
              home_dir: PathBuf,
                  args: Vec<String>,
           current_dir: PathBuf,
           current_exe: PathBuf,
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
    fn from_env() -> App {
        use std::env::*;
        let home_dir = home_dir().expect("failed to get the path of the current user's home directory");
        let args = args().collect();
        let current_dir = current_dir().expect("failed to get the current working directory");
        let current_exe = current_exe().expect("failed to get the full filesystem path of the current running executable");
        let home_dir_clone = home_dir.clone(); // TODO: See if this can be inlined
        App {
                        home_dir: home_dir,
                            args: args,
                     current_dir: current_dir,
                     current_exe: current_exe,
                         sbt_jar: PathBuf::new(),
                     sbt_version: Default::default(),
            sbt_explicit_version: Default::default(),
                         verbose: Default::default(),
                        java_cmd: "java".into(),
                  sbt_launch_dir: { let mut p = home_dir_clone; p.push(".sbt/launchers"); p },
                  extra_jvm_opts: Default::default(),
                       java_args: Default::default(),
                    sbt_commands: Default::default(),
                   residual_args: Default::default(),
                         sbt_new: Default::default(),
        }
    }

    // TODO: See if this can become a macro
    fn vlog(&self, s: &str) -> bool { if self.verbose { eprintln!("{}", s) }; self.verbose }

    fn script_name(&self) -> String {
        self.current_exe.file_name().unwrap().to_string_lossy().into_owned()
    }

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
                        eprintln!("\"{}\"", arg);
                    } else {
                        eprintln!("{}", arg);
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
            self.sbt_jar = PathBuf::from(&self.home_dir);
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
            script_name=self.script_name(),
            default_jvm_opts=self.default_jvm_opts().join(" "),
        );
    }

    fn run(&mut self) {
        fn require_arg(tpe: &str, opt: &str, arg: &str) {
            if arg.is_empty() || &arg[0..1] == "-" {
                die!("{} requires <{}> argument", opt, tpe);
            }
        }
        let args0 = self.args.clone(); // TODO: See if this clone can be avoided (or at least)
        let mut args = args0.iter().skip(1); // skip the path of the executable
        while let Some(arg) = args.next() {
            let mut next = || -> String { args.next().unwrap_or(&"".to_string()).to_string() };
            match arg.as_ref() {
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

        let argumentCount = self.residual_args.len();

        self.set_sbt_version();
        self.vlog(&format!("Detected sbt version {}", self.sbt_version));

        if argumentCount == 0 {
            self.vlog(&format!("Starting {}: invoke with -help for other options", self.script_name()));
            self.residual_args = vec!["shell".into()];
        }

        // verify this is an sbt dir
        if !File::open(PathBuf::from("build.sbt")).is_ok() && !PathBuf::from("project").is_dir() && !self.sbt_new {
            println!("{pwd} doesn't appear to be an sbt project.", pwd=self.current_dir.display());
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

fn make_lsp_json_str(method: &str, params: Value) -> Result<String, serde_json::error::Error> {
    let msg = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let request = serde_json::to_string(&msg)?;
    Ok(format!("Content-Length: {}\r\n\r\n{}", request.len(), request))
}

#[derive(Debug, PartialEq)]
/// A message header, as described in the Language Server Protocol specification.
enum LspHeader {
    ContentType,
    ContentLength(usize),
}

/// Given a reference to a reader, attempts to read a Language Server Protocol message,
/// blocking until a message is received.
pub fn read_message<B: BufRead>(reader: &mut B) -> Value {
    let mut buffer = String::new();
    let mut content_length: Option<usize> = None;

    // read in headers.
    loop {
            buffer.clear();
            reader.read_line(&mut buffer).unwrap();
            match &buffer {
                s if s.trim().len() == 0 => { break }, // empty line is end of headers
                s => {
                    match parse_header(s) {
                        LspHeader::ContentLength(len) => content_length = Some(len),
                        LspHeader::ContentType => (), // utf-8 only currently allowed value
                    };
                }
            };
        }

    let content_length = content_length.ok_or(format!("missing content-length header: {}", buffer)).unwrap();
    // message body isn't newline terminated, so we read content_length bytes
    let mut body_buffer = vec![0; content_length];
    reader.read_exact(&mut body_buffer).unwrap();
    let body = String::from_utf8(body_buffer).unwrap();
    serde_json::from_str::<Value>(&body).unwrap()
}

const HEADER_CONTENT_LENGTH: &'static str = "content-length";
const HEADER_CONTENT_TYPE: &'static str = "content-type";

/// Given a header string, attempts to extract and validate the name and value parts.
fn parse_header(s: &str) -> LspHeader {
    let split: Vec<String> = s.split(": ").map(|s| s.trim().to_lowercase()).collect();
    if split.len() != 2 { panic!(format!("malformed header: {}", s)) }
    match split[0].as_ref() {
        HEADER_CONTENT_TYPE   => LspHeader::ContentType,
        HEADER_CONTENT_LENGTH => LspHeader::ContentLength(usize::from_str_radix(&split[1], 10).unwrap()),
        _ => panic!(format!("Unknown header: {}", s)),
    }
}

fn handle_msg_quietly<B: BufRead>(mut reader: B) {
    match serde_json::from_value(read_message(&mut reader)).unwrap() {
        JsonRpc::Request(obj)    => eprintln!("client received unexpected request: {:?}", obj),
        JsonRpc::Notification(_) => (),
        JsonRpc::Success(_)      => (),
        JsonRpc::Error(obj)      => println!("recv error: {:?}", obj),
    }
}


fn handle_msg<B: BufRead>(mut reader: B) {
    match serde_json::from_value(read_message(&mut reader)).unwrap() {
        JsonRpc::Request(obj)      => eprintln!("client received unexpected request: {:?}", obj),
        JsonRpc::Notification(obj) => println!("recv notification: {:?}", obj),
        JsonRpc::Success(obj)      => println!("recv success: {:?}", obj),
        JsonRpc::Error(obj)        => println!("recv error: {:?}", obj),
    }
}

enum ExitCode { Success, Failure }

fn handle_msg_to_exit_code<B: BufRead>(mut reader: B) -> ExitCode {
    let mut done = false;
    let mut success = false;
    let mut failure = false;

    loop {
        let json_rpc = serde_json::from_value(read_message(&mut reader)).unwrap();
        match json_rpc {
            JsonRpc::Request(obj)          => eprintln!("client received unexpected request: {:?}", obj),
            JsonRpc::Success(obj)          => println!("recv success: {:?}", obj),
            JsonRpc::Error(obj)            => println!("recv error: {:?}", obj),
            JsonRpc::Notification(ref obj) => {
                match json_rpc.get_method() {
                    Some("window/logMessage") => {
                        let params0 = json_rpc.get_params().unwrap();
                        let params = match params0 {
                            jsonrpc_lite::Params::Array(_) => panic!("not expecting array"),
                            jsonrpc_lite::Params::None(()) => panic!("not expecting none"),
                            jsonrpc_lite::Params::Map(kvs) => kvs,
                        };
                        let lvl = params["type"].as_i64().unwrap();
                        let msg = params["message"].as_str().unwrap();
                        println!("{}", msg);
                        if msg == "Exited with code 0" { success = true }
                        if msg == "Done" { done = true }
                        if lvl == 1 { failure = true }
                    },
                    Some("textDocument/publishDiagnostics") => {
                        let params0 = json_rpc.get_params().unwrap();
                        let params = match params0 {
                            jsonrpc_lite::Params::Array(_) => panic!("not expecting array"),
                            jsonrpc_lite::Params::None(()) => panic!("not expecting none"),
                            jsonrpc_lite::Params::Map(kvs) => kvs,
                        };
                        let uri = params["uri"].as_str().unwrap();
                        let diagnostics = params["diagnostics"].as_array().unwrap();
                        for diagnostic in diagnostics {
                            println!("{}: {}", uri, diagnostic);
                        }
                    },
                    Some(_) | _ => println!("recv notification: {:?}", obj),
                }
            },
        }

        // val Error = 1L
        // val Warning = 2L
        // val Info = 3L
        // val Log = 4L

        // 'runMain t.Main'     reports "[log] Exited with code 0" and then "[log] Done"
        // 'runMain t.BadMain'  reports "[log] Done"               and then "[error] Nonzero exit code: 1"
        // 'compile' w/ error   reports "[error] Compilation failed"
        // 'compile' w/o errors reports "[log] Done", but we can't distinguish that from BadMain's "[log] Done"...

        if success && done { return ExitCode::Success }
        if failure { return ExitCode::Failure }
    }
}

fn talk_to_client() {
    use std::env::*;

    // TODO: Figure out a way to indicate the port file in the error message
    let portFile = File::open("project/target/active.json").expect("failed to open port file");
    let json: serde_json::Value = serde_json::de::from_reader(portFile).unwrap();
    let uri = json["uri"].as_str().unwrap();
    // TODO: Use a less idiotic way to get the path from the URI
    let socketFilePath = &uri[8..];

    let mut stream = UnixStream::connect(socketFilePath).unwrap();
    let json_str = make_lsp_json_str("initialize", json!({})).unwrap();
    stream.write_all(json_str.as_bytes()).unwrap();
    stream.flush().unwrap();

    let mut reader = BufReader::new(stream);
    handle_msg_quietly(&mut reader);

    let args1 = args().skip(1); // skip the path of the executable
    let commandLine = {
        // TODO: Make mk_string
        let mut s = args1.take(1).fold(String::new(), |acc, x| acc + &x + " ");
        let len = s.len() - 1;
        s.truncate(len);
        s
    };
    let json_str2 = make_lsp_json_str("sbt/exec", json!({"commandLine": commandLine})).unwrap();
    let mut stream2 = UnixStream::connect(socketFilePath).unwrap();
    stream2.write_all(json_str2.as_bytes()).unwrap();
    stream2.flush().unwrap();

    let reader2 = BufReader::new(stream2);
    match handle_msg_to_exit_code(reader2) {
        ExitCode::Failure => exit(1),
        ExitCode::Success => exit(0),
    }
}

fn main() {
    App::from_env().run();
}
