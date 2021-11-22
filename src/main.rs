//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//

use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Output};

use anyhow::Result;
use structopt::StructOpt;
use tempfile::tempdir;

#[derive(Debug, StructOpt)]
#[structopt(about = "docker to uefi bootable image")]
enum Args {
    Create {
        #[structopt(short, long)]
        image_name: String,

        #[structopt(short, long)]
        flavor: String,

        #[structopt(short, long, parse(from_os_str))]
        output_file: PathBuf,

        // Disk size in GB
        #[structopt(short, long, default_value = "8")]
        disk_size: usize,
    },
}

fn run(exe: String, args: &[String]) -> Result<Output, std::io::Error> {
    let mut cmd = Command::new(exe);

    for arg in args {
        cmd.arg(arg);
    }

    cmd.output()
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

struct LoopbackDevice {
    path: String,
}

impl LoopbackDevice {
    fn new(source_path: String) -> Result<Self> {
        let output = run(
            "losetup".into(),
            &["--show".into(), "--find".into(), source_path],
        )?;

        let mut path: String = output.stdout.iter().map(|x| *x as char).collect();
        if path.ends_with('\n') {
            path.pop();
        }

        Ok(Self { path })
    }

    fn path(&self) -> String {
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

fn main() -> Result<()> {
    let args = Args::from_args_safe()?;

    match args {
        Args::Create {
            image_name,
            flavor: _,
            output_file,
            disk_size,
        } => {
            println!(
                "Creating a bootable image {:?} out of {:?}",
                output_file, image_name,
            );

            let working_dir = tempdir()?;

            // Create blank file
            let img_path = {
                let mut img_path = working_dir.path().to_path_buf();
                img_path.push("output.img");
                img_path.into_os_string().into_string().unwrap()
            };
            println!("> Creating {} GB file at {:?}", disk_size, img_path,);

            let img = File::create(&img_path)?;
            img.set_len((disk_size * 1024 * 1024 * 1024).try_into().unwrap())?;
            drop(img);

            println!("> Create partitions");
            run(
                "sgdisk".into(),
                &[
                    "-n".into(),
                    "1:2048:4095".into(),
                    "-c".into(),
                    "1:\"BIOS Boot Partition\"".into(),
                    "-t".into(),
                    "1:ef02".into(),
                    img_path.clone(),
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
                    "1:ef00".into(),
                    img_path.clone(),
                ],
            )?;

            run(
                "sgdisk".into(),
                &["-n".into(), "3:413696:".into(), img_path.clone()],
            )?;

            println!("> Loopback main disk");
            let root_device = LoopbackDevice::new(img_path.clone())?;

            println!("> Main disk at {}", root_device.path());

            let root_device_partition_2 = format!("{}{}", root_device.path(), "p2");
            let root_device_partition_3 = format!("{}{}", root_device.path(), "p3");

            run("partprobe".into(), &[root_device.path()])?;

            println!("> Format partitions");
            run(
                "mkfs.vfat".into(),
                &["-F".into(), "32".into(), root_device_partition_2],
            )?;

            run("mkfs.ext4".into(), &[root_device_partition_3])?;

            // TODO: save container to mount

            // TODO: install extra packages in container to support UEFI boot

            // TODO: install bootloader

            // TODO: fix startup.nsh for debian

            // TODO: clean up

            println!("> Move {:?} to {:?}", img_path, output_file);
            std::fs::rename(img_path, output_file)?;
        }
    }

    Ok(())
}
