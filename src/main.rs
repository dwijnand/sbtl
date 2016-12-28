#![allow(non_upper_case_globals)]
//#![allow(dead_code)]

use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};

fn home() -> PathBuf {
    std::env::home_dir().unwrap()
}

macro_rules! echoerr(
    ($($arg:tt)*) => (writeln!(&mut ::std::io::stderr(), $($arg)*).expect("failed printing to stderr");)
);

fn sbt_launch_dir() -> PathBuf {
    let mut p = PathBuf::from(home());
    p.push(".sbt/launchers");
    p
}

fn build_props() -> PathBuf {
    PathBuf::from("project/build.properties")
}

fn build_props_sbt() -> String {
    if let Ok(f) = std::fs::File::open(build_props()) {
        let f = std::io::BufReader::new(f);

        use std::io::BufRead;
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
    let mut p = PathBuf::from(sbt_launch_dir());
    p.push(version);
    p.push("sbt-launch.jar");
    p
}

// TODO: Rename to sbt_launch_*, change line length to 112
const sbt_ivy_release_repo: &'static str = "http://repo.typesafe.com/typesafe/ivy-releases";
#[allow(dead_code)]
const sbt_ivy_snapshot_repo: &'static str = "https://repo.scala-sbt.org/scalasbt/ivy-snapshots";
const sbt_mvn_release_repo: &'static str = "http://repo.scala-sbt.org/scalasbt/maven-releases";
#[allow(dead_code)]
const sbt_mvn_snapshot_repo: &'static str = "http://repo.scala-sbt.org/scalasbt/maven-snapshots";

// TODO: Make rustfmt align =>
fn url_base(version: &str) -> &'static str {
    match version {
        s if s.starts_with("0.7.") => "http://simple-build-tool.googlecode.com",
        s if s.starts_with("0.10.") => sbt_ivy_release_repo,
        "0.11.1" | "0.11.2" => sbt_ivy_release_repo,
        // "0.*-yyyymmdd-hhMMss" => sbt_ivy_snapshot_repo
        s if s.starts_with("0.") => sbt_ivy_release_repo,
        // "*-yyyymmdd-hhMMss" => sbt_mvn_snapshot_repo
        _ => sbt_mvn_release_repo,
    }
}

fn make_url(version: &str) -> String {
    let base = url_base(version);

    let url1 = format!("{}/org.scala-tools.sbt/sbt-launch/{}/sbt-launch.jar", base, version);
    let url2 = format!("{}/org.scala-sbt/sbt-launch/{}/sbt-launch.jar", base, version);
    let url3 = format!("{}/org/scala-sbt/sbt-launch/{}/sbt-launch.jar", base, version);

    match version {
        s if s.starts_with("0.7.") => format!("{}/files/sbt-launch-0.7.7.jar", base),
        s if s.starts_with("0.10.") => url1,
        "0.11.1" | "0.11.2" => url1,
        s if s.starts_with("0.") => url2,
        _ => url3,
    }
}

fn download_url(sbt_version: &str, url: &str, jar: &Path) {
    echoerr!("Downloading sbt launcher for {}:", sbt_version);
    echoerr!("  From  {}", url);
    echoerr!("    To  {}", jar.display());
    std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
    extern crate hyper;
    let mut c = hyper::client::Client::new();
    c.set_redirect_policy(hyper::client::RedirectPolicy::FollowAll);
    let rb = c.get(url);
    let rsp = rb.send().unwrap();
    let mut reader = std::io::BufReader::new(rsp);
    let f = std::fs::File::create(jar).unwrap();
    let mut writer = std::io::BufWriter::new(&f);
    let mut buf = [0; 16384];
    use std::io::Read;
    while {
        let bc = reader.read(&mut buf).unwrap();
        if bc > 0 {
            writer.write(&buf[0..bc]).unwrap();
        };
        bc > 0
    } {}
    writer.flush().unwrap();
}

fn exec_runner<S: AsRef<OsStr>>(args: &[S]) {
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(&args[0]).args(&args[1..]).exec();
    println!("error: {}", err);
    if let Some(err) = err.raw_os_error() {
        std::process::exit(err);
    }
    std::process::exit(-1)
}

fn main() {
    let sbt_version = build_props_sbt();
    let sbt_jar = {
        let mut sbt_jar = jar_file(&sbt_version);
        if !sbt_jar.exists() {
            sbt_jar = PathBuf::from(home());
            sbt_jar.push(format!(".ivy2/local/org.scala-sbt/sbt-launch/{}/jars/sbt-launch.jar", sbt_version));
        }
        if !sbt_jar.exists() {
            sbt_jar = jar_file(&sbt_version);
            download_url(&sbt_version, &make_url(&sbt_version), &sbt_jar);
        }
        sbt_jar
    };

    let java_cmd = "java";
    let extra_jvm_opts = ["-Xms512m", "-Xmx1536m", "-Xss2m"];
    let java_args: [&str; 0] = [];
    let sbt_commands: [&str; 0] = [];
    let residual_args = ["shell"];

    let exec_args: Vec<&OsStr> = {
        let mut exec_args = Vec::new();
        exec_args.push(java_cmd.as_ref());
        exec_args.extend_from_slice(&extra_jvm_opts.iter().map(|x| x.as_ref()).collect::<Vec<_>>());
        exec_args.extend_from_slice(&java_args.iter().map(|x| x.as_ref()).collect::<Vec<_>>());
        exec_args.extend_from_slice(&["-jar".as_ref(), sbt_jar.as_ref()]);
        exec_args.extend_from_slice(&sbt_commands.iter().map(|x| x.as_ref()).collect::<Vec<_>>());
        exec_args.extend_from_slice(&residual_args.iter().map(|x| x.as_ref()).collect::<Vec<_>>());
        exec_args
    };

    exec_runner(&exec_args)
}
