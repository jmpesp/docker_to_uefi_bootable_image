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
            let mut img_path = working_dir.path().to_path_buf();
            img_path.push("output.img");
            println!(
                "> Creating {} GB file at {:?}",
                disk_size,
                img_path.as_path()
            );

            let mut img = File::create(&img_path)?;
            img.set_len((disk_size * 1024 * 1024 * 1024).try_into().unwrap());

            // TODO: create partitions

            // TODO: format partitions

            // TODO: loopback mount partitions

            // TODO: save container to mount

            // TODO: install extra packages in container to support UEFI boot

            // TODO: install bootloader

            // TODO: fix startup.nsh for debian

            // TODO: clean up

            println!("> Move {:?} to {:?}", img_path.as_path(), output_file);
            drop(img);
            std::fs::rename(img_path, output_file)?;
        }
    }

    Ok(())
}
