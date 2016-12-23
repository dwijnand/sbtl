use std::ffi::OsStr;
use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::home_dir().unwrap()
}

fn sbt_launch_dir() -> PathBuf {
    let mut p = PathBuf::from(home());
    p.push(".sbt/launchers");
    p
}

fn jar_file(sbt_version: &str) -> PathBuf {
    let mut p = PathBuf::from(sbt_launch_dir());
    p.push(sbt_version);
    p.push("sbt-launch.jar");
    p
}

fn sbt_version() -> String {
    "0.13.13".to_string()
}

fn sbt_jar() -> PathBuf {
    jar_file(&sbt_version())
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
    let sbt_jar = sbt_jar();

    let java_cmd = OsStr::new("java");
    let extra_jvm_opts = [OsStr::new("-Xms512m"), OsStr::new("-Xmx1536m"), OsStr::new("-Xss2m")];
    let java_args: [&OsStr; 0] = [];
    let sbt_commands: [&OsStr; 0] = [];
    let residual_args = [OsStr::new("shell")];

    let mut exec_args: Vec<&OsStr> = Vec::new();
    exec_args.push(java_cmd);
    exec_args.extend_from_slice(&extra_jvm_opts);
    exec_args.extend_from_slice(&java_args);
    exec_args.extend_from_slice(&[OsStr::new("-jar"), sbt_jar.as_os_str()]);
    exec_args.extend_from_slice(&sbt_commands);
    exec_args.extend_from_slice(&residual_args);
    let exec_args = exec_args;

    exec_runner(&exec_args)
}
