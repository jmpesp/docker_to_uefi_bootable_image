use std::io::Result;
use std::process::{Command, Output};

fn run(exe: String, args: &[String]) -> Result<Output> {
    let mut cmd = Command::new(exe);

    for arg in args {
        cmd.arg(arg);
    }

    cmd.output()
}

fn main() -> Result<()> {
    let result = run("ls".into(), &["-al".into()])?;

    println!("{:?}", result.status);
    assert!(result.status.success());
    assert_eq!(result.status.code().unwrap(), 0);

    let text = result
        .stdout
        .iter()
        .map(|x| (*x as char).to_string())
        .collect::<Vec<String>>()
        .join("");
    println!("{}", text);

    Ok(())
}
