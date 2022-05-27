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

use structopt::StructOpt;

use docker_to_uefi_bootable_image::*;

#[derive(Debug, StructOpt)]
#[structopt(about = "docker to uefi bootable image")]
enum Args {
    Create {
        #[structopt(short, long)]
        image_name: String,

        #[structopt(short, long, parse(from_os_str))]
        output_file: PathBuf,

        // Disk size in GB
        #[structopt(short, long, default_value = "8")]
        disk_size: usize,

        // Optional root password
        #[structopt(short, long)]
        root_passwd: Option<String>,

        #[structopt(short, long, use_delimiter = true)]
        extra_packages: Vec<String>,

        // OS flavor (debian, ubuntu, ...)
        #[structopt(short, long)]
        flavor: String,
    },
}

fn main() -> Result<()> {
    let args = Args::from_args_safe()?;

    match args {
        Args::Create {
            image_name,
            output_file,
            disk_size,
            root_passwd,
            extra_packages,
            flavor,
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
                    "2:ef00".into(),
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
                format!("{}/boot/efi", mount_root_path),
            )?;

            run(
                "mkdir".into(),
                &[
                    "-p".into(),
                    format!("{}/boot/efi/EFI/BOOT/", mount_root_path),
                ],
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
                    "--entrypoint=/bin/sh".into(),
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
            run("docker".into(), &["stop".into(), tempname.clone()])?;
            run("docker".into(), &["rm".into(), tempname])?;

            run(
                "tar".into(),
                &[
                    "--sparse".into(),
                    "-C".into(),
                    mount_partition_3.dest(),
                    "-xf".into(),
                    export_path,
                ],
            )?;

            println!("> install extra packages in container to support UEFI boot");

            std::fs::copy(
                "/etc/resolv.conf",
                format!("{}/etc/resolv.conf", mount_partition_3.dest()),
            )?;

            let bind_dev = Mount::bind("/dev".into(), format!("{}/dev", mount_partition_3.dest()))?;
            let bind_proc =
                Mount::bind("/proc".into(), format!("{}/proc", mount_partition_3.dest()))?;
            let bind_sys = Mount::bind("/sys".into(), format!("{}/sys", mount_partition_3.dest()))?;

            run(
                "chroot".into(),
                &[
                    mount_partition_3.dest(),
                    "apt".into(),
                    "update".into(),
                    "-y".into(),
                ],
            )?;

            // stop to manually chroot and debug
            //println!("> Enter some text when done");
            //let mut s = String::new();
            //std::io::stdin().read_line(&mut s).expect("Not a string?");

            let kernel_pkg = match flavor.as_ref() {
                "debian" => "linux-image-amd64",
                "ubuntu" => "linux-image-generic",
                _ => {
                    bail!("flavor not supported!");
                }
            };

            run(
                "chroot".into(),
                &[
                    mount_partition_3.dest(),
                    "apt".into(),
                    "install".into(),
                    "-y".into(),
                    kernel_pkg.into(),
                    "systemd-sysv".into(),
                    "grub2-common".into(),
                    "grub-efi-amd64-bin".into(),
                    "initramfs-tools".into(),
                ],
            )?;

            if !extra_packages.is_empty() {
                println!("> install extra packages");

                let mut args = vec![
                    mount_partition_3.dest(),
                    "apt".into(),
                    "install".into(),
                    "-y".into(),
                ];
                args.extend_from_slice(&extra_packages[..]);

                run("chroot".into(), &args)?;
            }

            println!("> write fstab");

            let mut fstab = File::create(format!("{}/etc/fstab", mount_partition_3.dest()))?;

            let p3_fs_uuid: String = output_stdout_string(&run(
                "blkid".into(),
                &["-o".into(), "export".into(), root_device_partition_3],
            )?)
            .split('\n')
            .filter(|x| x.starts_with("UUID="))
            .collect();

            writeln!(fstab, "{} / ext4 errors=remount-ro 0 1", p3_fs_uuid)?;

            let p2_fs_uuid: String = output_stdout_string(&run(
                "blkid".into(),
                &["-o".into(), "export".into(), root_device_partition_2],
            )?)
            .split('\n')
            .filter(|x| x.starts_with("UUID="))
            .collect();

            writeln!(fstab, "{} /boot/efi vfat defaults 0 2", p2_fs_uuid)?;

            drop(fstab);

            run(
                "cat".into(),
                &[format!("{}/etc/fstab", mount_partition_3.dest())],
            )?;

            println!("> install grub");

            run(
                "mkdir".into(),
                &[
                    "-p".into(),
                    format!("{}/boot/grub/", mount_partition_3.dest()),
                ],
            )?;

            let mut device_map =
                File::create(format!("{}/boot/grub/device.map", mount_partition_3.dest()))?;
            writeln!(device_map, "(hd0) {}", root_device.path())?;
            drop(device_map);

            run(
                "mkdir".into(),
                &[
                    "-p".into(),
                    format!("{}/etc/default/", mount_partition_3.dest()),
                ],
            )?;

            let mut grub_file =
                File::create(format!("{}/etc/default/grub", mount_partition_3.dest()))?;
            writeln!(grub_file, "GRUB_DEVICE={}", p3_fs_uuid)?;
            writeln!(
                grub_file,
                "GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash console=tty0 console=ttyS0,115200\"",
            )?;
            drop(grub_file);

            run(
                "grub-install".into(),
                &[
                    "--target=x86_64-efi".into(),
                    format!("--efi-directory={}/boot/efi/", mount_partition_3.dest()),
                    format!("--root-directory={}", mount_partition_3.dest()),
                    "--no-floppy".into(),
                    root_device.path(),
                ],
            )?;
            run(
                "chroot".into(),
                &[
                    mount_partition_3.dest(),
                    "grub-mkconfig".into(),
                    "-o".into(),
                    "/boot/grub/grub.cfg".into(),
                ],
            )?;

            println!("> no loop necessary in final image");
            run(
                "chroot".into(),
                &[
                    mount_partition_3.dest(),
                    "rm".into(),
                    "/boot/grub/device.map".into(),
                ],
            )?;

            println!("> update-initramfs");
            run(
                "chroot".into(),
                &[
                    mount_partition_3.dest(),
                    "update-initramfs".into(),
                    "-u".into(),
                ],
            )?;

            let root_passwd: String = if let Some(v) = root_passwd {
                v
            } else {
                rand::thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take(16)
                    .map(char::from)
                    .collect()
            };

            println!("> set root password as {}", root_passwd);

            let mut passwd = Command::new("chroot")
                .stdin(std::process::Stdio::piped())
                .arg(mount_partition_3.dest())
                .arg("passwd")
                .spawn()?;

            {
                let passwd_stdin = passwd.stdin.as_mut().unwrap();
                writeln!(passwd_stdin, "{}", root_passwd)?;
                writeln!(passwd_stdin, "{}", root_passwd)?;
            }

            passwd.wait_with_output()?;

            println!("> Clean up");
            drop(bind_dev);
            drop(bind_proc);
            drop(bind_sys);
            drop(mount_partition_2);
            drop(mount_partition_3);

            println!("> Move {:?} to {:?}", img_path, output_file);
            std::fs::rename(img_path, output_file)?;
        }
    }

    Ok(())
}
