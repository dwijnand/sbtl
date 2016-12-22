fn main() {
    let home = std::env::var("HOME").unwrap();
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new("java")
        .args(&["-Xms512m", "-Xmx1536m", "-Xss2m"])
        .args(&["-jar", &format!("{}/.sbt/launchers/0.13.13/sbt-launch.jar", home)])
        .arg("shell")
        .exec();
    println!("error: {}", err);
    if let Some(err) = err.raw_os_error() {
        std::process::exit(err);
    }
    std::process::exit(-1)
}
