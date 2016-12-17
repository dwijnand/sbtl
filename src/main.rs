use std::process::Command;
use std::os::unix::process::CommandExt;

fn main() {
    Command::new("java")
        .arg("-Xms512m")
        .arg("-Xmx1536m")
        .arg("-Xss2m")
        .args(&["-jar", "/Users/dnw/.sbt/launchers/0.13.13/sbt-launch.jar"])
        .arg("shell")
        .exec();
}
