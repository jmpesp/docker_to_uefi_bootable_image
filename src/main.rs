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

    // Debug: print what is about to run
    println!("# {:?}", cmd);

    let result = cmd.output()?;

    // Debug: print output
    let text = result
        .stdout
        .iter()
        .map(|x| (*x as char).to_string())
        .collect::<String>();

    println!("# {}", text);

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

struct Mount {
    source: String,
    dest: String,
}

impl Mount {
    fn new(source: String, dest: String) -> Result<Self> {
        run("mkdir".into(), &["-p".into(), dest.clone()])?;

        println!(">> mount {} {}", source, dest);
        run("mount".into(), &[source.clone(), dest.clone()])?;

        Ok(Self { source, dest })
    }

    fn dest(&self) -> String {
        self.dest.clone()
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        println!("# Umount {}", self.dest);
        run("umount".into(), &[self.dest.clone()]).expect("could not umount!");
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
                &["-F".into(), "32".into(), root_device_partition_2.clone()],
            )?;

            run("mkfs.ext4".into(), &[root_device_partition_3.clone()])?;

            println!("> Mount partitions");

            let mount_root_path = {
                let mut path = working_dir.path().to_path_buf();
                path.push("mnt");
                path.into_os_string().into_string().unwrap()
            };

            let mount_partition_3 =
                Mount::new(root_device_partition_3.clone(), mount_root_path.clone())?;

            let mount_partition_2 = Mount::new(
                root_device_partition_2.clone(),
                format!("{}/efi/EFI/BOOT/", mount_root_path),
            )?;

            println!("> Copy docker image contents to directory");

            let tempname: String = uuid::Uuid::new_v4().to_string();

            let export_path = {
                let mut path = working_dir.path().to_path_buf();
                path.push("export.tar");
                path.into_os_string().into_string().unwrap()
            };

            run(
                "docker".into(),
                &[
                    "run".into(),
                    "-d".into(),
                    "--name".into(),
                    tempname.clone(),
                    image_name,
                ],
            )?;
            run(
                "docker".into(),
                &[
                    "export".into(),
                    "-o".into(),
                    export_path.clone(),
                    tempname.clone(),
                ],
            )?;
            run("docker".into(), &["rm".into(), tempname])?;

            run(
                "tar".into(),
                &[
                    "-C".into(),
                    mount_partition_3.dest(),
                    "-xf".into(),
                    export_path,
                ],
            )?;

            // Debug: output what's in mount partitions
            run("ls".into(), &["-al".into(), mount_partition_3.dest()])?;
            run("ls".into(), &["-al".into(), mount_partition_2.dest()])?;

            // TODO: install extra packages in container to support UEFI boot

            // TODO: install bootloader

            // TODO: fix startup.nsh for debian

            println!("> Clean up");
            drop(mount_partition_2);
            drop(mount_partition_3);

            println!("> Move {:?} to {:?}", img_path, output_file);
            std::fs::rename(img_path, output_file)?;
        }
    }

    Ok(())
}
