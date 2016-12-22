fn main() {
    let home = std::env::home_dir().unwrap();
    let sbt_jar;
    {
        let mut p = std::path::PathBuf::from(home);
        p.push(".sbt/launchers/0.13.13/sbt-launch.jar");
        sbt_jar = p;
    }
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new("java")
        .args(&["-Xms512m", "-Xmx1536m", "-Xss2m"])
        .args(&[&"-jar".as_ref(), &sbt_jar.as_os_str()])
        .arg("shell")
        .exec();
    println!("error: {}", err);
    if let Some(err) = err.raw_os_error() {
        std::process::exit(err);
    }
    std::process::exit(-1)
}
