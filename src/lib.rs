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

pub struct LoopbackDisk {
    working_dir: tempfile::TempDir,
    img_path: String,
    root_device: LoopbackDevice,
    size_in_gb: usize,
}

impl LoopbackDisk {
    pub fn new(size_in_gb: usize) -> Result<Self> {
        let working_dir = tempdir()?;

        // Create blank file
        let img_path = {
            let mut img_path = working_dir.path().to_path_buf();
            img_path.push("output.img");
            if let Ok(s) = img_path.into_os_string().into_string() {
                s
            } else {
                bail!("img_path.into_os_string().into_string()");
            }
        };

        let img = File::create(&img_path)?;
        img.set_len((size_in_gb * 1024 * 1024 * 1024).try_into()?)?;
        drop(img);

        let root_device = LoopbackDevice::new(img_path.clone())?;

        Ok(Self {
            working_dir,
            img_path,
            root_device,
            size_in_gb,
        })
    }

    pub fn path(&self) -> String {
        self.root_device.path()
    }

    pub fn img_path(&self) -> String {
        self.img_path.clone()
    }
}

pub struct PartitionedLoopbackDisk {
    loopback_disk: LoopbackDisk,
}

impl PartitionedLoopbackDisk {
    /// Consume a LoopbackDisk, produce a PartitionedLoopbackDisk
    pub fn from(loopback_disk: LoopbackDisk) -> Result<Self> {
        run(
            "sgdisk".into(),
            &[
                "-n".into(),
                "1:2048:4095".into(),
                "-c".into(),
                "1:\"BIOS Boot Partition\"".into(),
                "-t".into(),
                "1:ef02".into(),
                loopback_disk.path(),
            ],
        )?;

        run(
            "sgdisk".into(),
            &[
                "-n".into(),
                "2:4096:413695".into(),
                "-c".into(),
                "2:\"EFI System Partition\"".into(),
                "-t".into(),
                "2:ef00".into(),
                loopback_disk.path(),
            ],
        )?;

        run(
            "sgdisk".into(),
            &["-n".into(), "3:413696:".into(), loopback_disk.path()],
        )?;

        run("partprobe".into(), &[loopback_disk.path()])?;

        Ok(Self { loopback_disk })
    }

    pub fn path(&self) -> String {
        self.loopback_disk.path()
    }

    pub fn working_dir(&self) -> &tempfile::TempDir {
        &self.loopback_disk.working_dir
    }

    pub fn img_path(&self) -> String {
        self.loopback_disk.img_path()
    }
}

/*
// TODO needs root
#[test]
fn partition_disk() {
    let dev = LoopbackDisk::new(1).unwrap();
    let partitioned_disk = PartitionedLoopbackDisk::from(dev).unwrap();
}
*/
