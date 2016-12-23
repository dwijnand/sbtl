use std::ffi::{OsStr, OsString};

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

    let extra_jvm_opts = [OsStr::new("-Xms512m").to_os_string(),
                          OsStr::new("-Xmx1536m").to_os_string(),
                          OsStr::new("-Xss2m").to_os_string()];
    let java_args: [OsString; 0] = [];
    let sbt_commands: [OsString; 0] = [];
    let residual_args: [OsString; 0] = [];

    let mut exec_args: Vec<OsString> = Vec::new();
    exec_args.push(OsStr::new("java").to_os_string());
    exec_args.extend_from_slice(&extra_jvm_opts);
    exec_args.extend_from_slice(&java_args);
    exec_args.extend_from_slice(&[OsStr::new("-jar").to_os_string(), sbt_jar.into_os_string()]);
    exec_args.push(OsStr::new("shell").to_os_string());
    exec_args.extend_from_slice(&sbt_commands);
    exec_args.extend_from_slice(&residual_args);
    let exec_args = exec_args;

    exec_runner(&exec_args)
}
