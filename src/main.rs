#![allow(non_upper_case_globals)]
//#![allow(dead_code)]

use std::ffi::OsStr;
use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

fn build_props() -> PathBuf { PathBuf::from("project/build.properties") }

const sbt_launch_ivy_release_repo: &'static str = "http://repo.typesafe.com/typesafe/ivy-releases";
const sbt_launch_mvn_release_repo: &'static str = "http://repo.scala-sbt.org/scalasbt/maven-releases";

fn home()           -> PathBuf { std::env::home_dir().unwrap() }
fn sbt_launch_dir() -> PathBuf { let mut p = PathBuf::from(home()); p.push(".sbt/launchers"); p }

macro_rules! echoerr(($($arg:tt)*) => (writeln!(&mut ::std::io::stderr(), $($arg)*).unwrap();));

fn build_props_sbt() -> String {
    if let Ok(f) = File::open(build_props()) {
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
    let mut p = PathBuf::from(sbt_launch_dir());
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

fn download_url(sbt_version: &str, url: &str, jar: &Path) {
    echoerr!("Downloading sbt launcher for {}:", sbt_version);
    echoerr!("  From  {}", url);
    echoerr!("    To  {}", jar.display());

    std::fs::create_dir_all(jar.parent().unwrap()).unwrap();

    extern crate hyper;
    let mut r = BufReader::new(hyper::client::Client::new().get(url).send().unwrap());
    let mut buf = [0; 16384];
    let mut jar = std::io::BufWriter::new(File::create(jar).unwrap());
    while {
        let bc = r.read(&mut buf).unwrap();
        jar.write(&buf[0..bc]).unwrap();
        bc > 0
    } {}
    jar.flush().unwrap();
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
