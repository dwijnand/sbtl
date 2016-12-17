use std::process::Command;
use std::process::exit;
use std::os::unix::process::CommandExt;

fn main() {
    let err = Command::new("java")
        .arg("-Xms512m")
        .arg("-Xmx1536m")
        .arg("-Xss2m")
        .args(&["-jar", "/Users/dnw/.sbt/launchers/0.13.13/sbt-launch.jar"])
        .arg("shell")
        .exec();
    println!("error: {}", err);
    if let Some(err) = err.raw_os_error() {
        exit(err);
    }
    exit(-1)
}
