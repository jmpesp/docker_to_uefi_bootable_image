//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output};

use anyhow::{bail, Result};
use rand::{distributions::Alphanumeric, Rng};
use tempfile::tempdir;

pub fn output_stdout_string(output: &Output) -> String {
    let mut text = output
        .stdout
        .iter()
        .map(|x| (*x as char).to_string())
        .collect::<String>();

    if text.ends_with('\n') {
        text.pop();
    }

    text
}

pub fn output_stderr_string(output: &Output) -> String {
    let mut text = output
        .stderr
        .iter()
        .map(|x| (*x as char).to_string())
        .collect::<String>();

    if text.ends_with('\n') {
        text.pop();
    }

    text
}

pub fn run(exe: String, args: &[String]) -> Result<Output> {
    run_with_env(exe, args, &[])
}

pub fn run_with_env(exe: String, args: &[String], env_vars: &[(String, String)]) -> Result<Output> {
    let mut cmd = Command::new(exe);

    for arg in args {
        cmd.arg(arg);
    }

    for env_var in env_vars {
        cmd.env(&env_var.0, &env_var.1);
    }

    // Debug: print what is about to run
    println!("# {:?} {:?}", cmd, env_vars);

    let result = cmd.output()?;

    // Debug: print output
    println!("# {}", output_stdout_string(&result));

    if !result.status.success() {
        bail!("Command failed!\n{}", output_stderr_string(&result));
    }

    Ok(result)
}

#[test]
fn test_run() -> Result<()> {
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

#[test]
fn grep() -> Result<()> {
    let result = run(
        "blkid".into(),
        &["-o".into(), "export".into(), "/dev/nvme0n1p1".into()],
    )?;

    let text = output_stdout_string(&result);

    let text: Vec<&str> = text
        .split("\n")
        .filter(|x| x.starts_with("UUID="))
        .collect();

    assert_eq!(text, vec!["UUID=D466-DF85"]);

    Ok(())
}

pub struct LoopbackDevice {
    path: String,
}

impl LoopbackDevice {
    pub fn new(source_path: String) -> Result<Self> {
        let output = run(
            "losetup".into(),
            &["--show".into(), "--find".into(), source_path],
        )?;

        let path: String = output_stdout_string(&output);

        Ok(Self { path })
    }

    pub fn path(&self) -> String {
        self.path.clone()
    }
}

impl Drop for LoopbackDevice {
    fn drop(&mut self) {
        println!("# Dropping {}", self.path);

        // XXX if your OS auto-mounted this, need a umount
        run("losetup".into(), &["-d".into(), self.path.clone()]).expect("could not drop!");
    }
}

pub struct Mount {
    dest: String,
}

impl Mount {
    pub fn new(source: String, dest: String) -> Result<Self> {
        run("mkdir".into(), &["-p".into(), dest.clone()])?;

        println!(">> mount {} {}", source, dest);
        run("mount".into(), &[source, dest.clone()])?;

        Ok(Self { dest })
    }

    pub fn bind(source: String, dest: String) -> Result<Self> {
        run("mkdir".into(), &["-p".into(), dest.clone()])?;

        println!(">> mount --bind {} {}", source, dest);
        run("mount".into(), &["--bind".into(), source, dest.clone()])?;

        Ok(Self { dest })
    }

    pub fn dest(&self) -> String {
        self.dest.clone()
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        println!("# Umount {}", self.dest);
        run("sync".into(), &[]).expect("could not sync!");
        run("umount".into(), &[self.dest.clone()]).expect("could not umount!");
    }
}
