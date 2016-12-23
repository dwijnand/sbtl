use std::ffi::OsStr;

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
    let home = std::env::home_dir().unwrap();

    let mut sbt_jar = std::path::PathBuf::from(home);
    sbt_jar.push(".sbt/launchers/0.13.13/sbt-launch.jar");
    let sbt_jar = sbt_jar;

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
