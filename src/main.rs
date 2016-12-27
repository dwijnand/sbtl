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

    let java_cmd = "java";
    let extra_jvm_opts = ["-Xms512m", "-Xmx1536m", "-Xss2m"];
    let java_args: [&str; 0] = [];
    let sbt_commands: [&str; 0] = [];
    let residual_args = ["shell"];

    let mut exec_args: Vec<&OsStr> = Vec::new();
    exec_args.push(java_cmd.as_ref());
    exec_args.extend_from_slice(&extra_jvm_opts.iter().map(|x| x.as_ref()).collect::<Vec<_>>());
    exec_args.extend_from_slice(&java_args.iter().map(|x| x.as_ref()).collect::<Vec<_>>());
    exec_args.extend_from_slice(&["-jar".as_ref(), sbt_jar.as_ref()]);
    exec_args.extend_from_slice(&sbt_commands.iter().map(|x| x.as_ref()).collect::<Vec<_>>());
    exec_args.extend_from_slice(&residual_args.iter().map(|x| x.as_ref()).collect::<Vec<_>>());
    let exec_args = exec_args;

    exec_runner(&exec_args)
}
