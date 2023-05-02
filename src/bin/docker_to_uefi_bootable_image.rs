//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Result};
use rand::{distributions::Alphanumeric, Rng};

use clap::{Parser, ValueEnum};

use docker_to_uefi_bootable_image::*;

#[derive(Debug, Parser)]
#[clap(about = "docker to uefi bootable image")]
enum Args {
    Create {
        #[clap(short, long)]
        image_name: String,

        #[clap(short, long)]
        output_file: PathBuf,

        // Disk size in GB
        #[clap(short, long, default_value = "8")]
        disk_size: usize,

        // Optional root password
        #[clap(short, long)]
        root_passwd: Option<String>,

        #[clap(short, long, value_delimiter = ',')]
        extra_packages: Vec<String>,

        // OS flavor (debian, ubuntu, ...)
        #[clap(short, long)]
        flavor: OsFlavor,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum OsFlavor {
    Debian,
    Ubuntu,
    Alpine,
}

fn main() -> Result<()> {
    let args = Args::parse();

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

            println!("> Creating {} GB blank disk", disk_size);
            let blank_disk = LoopbackDisk::new(disk_size)?;

            println!("> Creating partitioned disk");
            let partitioned_disk = PartitionedLoopbackDisk::from(blank_disk)?;

            println!("> Main disk at {}", partitioned_disk.path());

            let root_device_partition_2 = format!("{}{}", partitioned_disk.path(), "p2");
            let root_device_partition_3 = format!("{}{}", partitioned_disk.path(), "p3");

            println!("> Format partitions");
            run(
                "mkfs.vfat".into(),
                &["-F".into(), "32".into(), root_device_partition_2.clone()],
            )?;

            run("mkfs.ext4".into(), &[root_device_partition_3.clone()])?;

            println!("> Mount partitions");

            let mount_root_path = {
                let mut path = partitioned_disk.working_dir().path().to_path_buf();
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
                let mut path = partitioned_disk.working_dir().path().to_path_buf();
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

            // Update package repos
            match flavor {
                OsFlavor::Debian | OsFlavor::Ubuntu => {
                    run(
                        "chroot".into(),
                        &[
                            mount_partition_3.dest(),
                            "apt".into(),
                            "update".into(),
                            "-y".into(),
                        ],
                    )?;
                }

                OsFlavor::Alpine => {
                    run(
                        "chroot".into(),
                        &[mount_partition_3.dest(), "apk".into(), "update".into()],
                    )?;
                }
            }

            // stop to manually chroot and debug
            //println!("> Enter some text when done");
            //let mut s = String::new();
            //std::io::stdin().read_line(&mut s).expect("Not a string?");

            // Install necessary installer packages for EFI
            match flavor {
                OsFlavor::Debian | OsFlavor::Ubuntu => {
                    let kernel_pkg = match flavor {
                        OsFlavor::Debian => "linux-image-amd64",
                        OsFlavor::Ubuntu => "linux-image-generic",
                        _ => panic!("wat"),
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

                    // If Debian or Ubuntu, install extra packages - there isn't
                    // separate disk like Alpine.
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
                }

                OsFlavor::Alpine => {
                    run(
                        "chroot".into(),
                        &[
                            mount_partition_3.dest(),
                            "apk".into(),
                            "add".into(),
                            "grub-efi".into(),
                            "mkinitfs".into(),
                            "alpine-conf".into(),
                            "busybox-openrc".into(),
                        ],
                    )?;

                    // After installing busybox-openrc, set a drop command so we can umount later
                    let drop_command = DropCommand::new(
                        "chroot".into(),
                        vec![mount_partition_3.dest(), "openrc".into(), "shutdown".into()],
                    );

                    // Populate /answers for setup-alpine
                    let mut answers =
                        File::create(format!("{}/answers", mount_partition_3.dest()))?;

                    // See: https://github.com/alpinelinux/alpine-conf/blob/master/setup-alpine.in
                    writeln!(
                        answers,
                        r##"
KEYMAPOPTS="us us"
HOSTNAMEOPTS="alpine"
DEVDOPTS="mdev"
INTERFACESOPTS="
auto lo
iface lo inet loopback

auto eth0
iface eth0 inet dhcp
    hostname alpine
"
DNSOPTS="-d example.com 8.8.8.8"
TIMEZONEOPTS="UTC"
APKREPOSOPTS="-1"
USEROPTS="-a -u -g audio,video,netdev alpine"
SSHDOPTS="openssh"
NTPOPTS="chrony"
DISKOPTS="-m sys -k virt /tmp/mnt_loop/"
"##,
                    )?;

                    drop(answers);

                    // insane but it works
                    /*
                    run(
                        "chroot".into(),
                        &[
                            mount_partition_3.dest(),
                            "mkdir".into(),
                            "/tmp/mnt_loop".into(),
                        ],
                    )?;

                    let bind_mnt_loop = Mount::bind(
                        mount_partition_3.dest(),
                        format!("{}/tmp/mnt_loop", mount_partition_3.dest()),
                    )?;

                    run(
                        "chroot".into(),
                        &[
                            mount_partition_3.dest(),
                            "ls".into(),
                            "/tmp/mnt_loop".into(),
                        ],
                    )?;
                    */

                    //println!("> Enter some text when done");
                    //let mut s = String::new();
                    //std::io::stdin().read_line(&mut s).expect("Not a string?");

                    // Run setup-alpine in quick mode
                    //
                    // Note: this starts crond and acpid, which will hold open files under /dev/,
                    // preventing unmounts! that's why we have the openrc shutdown drop command.
                    run_with_env(
                        "chroot".into(),
                        &[
                            mount_partition_3.dest(),
                            "/bin/sh".into(),
                            "-x".into(), // XXX -e fails!
                            "/sbin/setup-alpine".into(),
                            "-e".into(), // empty root password
                            "-f".into(),
                            "-q".into(),
                            "/answers".into(),
                        ],
                        &[
                            ("USE_EFI".into(), "1".into()),
                            ("BOOTLOADER".into(), "none".into()),
                        ],
                    )?;

                    //drop(bind_mnt_loop);

                    //run(
                    //    "chroot".into(),
                    //    &[mount_partition_3.dest(), "rm".into(), "/answers".into()],
                    //)?;

                    //run(
                    //    "chroot".into(),
                    //    &[mount_partition_3.dest(), "mkdir".into(), "/sysroot".into()],
                    //)?;

                    // install what's in /etc/apk/world
                    let world_lines: Vec<String> = std::fs::read_to_string(format!(
                        "{}/etc/apk/world",
                        mount_partition_3.dest(),
                    ))?
                    .split('\n')
                    .map(|x| x.to_string())
                    .filter(|x| !x.is_empty())
                    .collect();

                    run(
                        "chroot".into(),
                        &vec![
                            vec![
                                mount_partition_3.dest(),
                                "apk".into(),
                                "add".into(),
                                // XXX --root /tmp/mnt_loop --overlay-from-stdin
                                //"--initdb".into(),
                                "--update-cache".into(),
                                "--clean-protected".into(),
                                // XXX from /etc/apk/repositories instead
                                "--repository".into(),
                                "https://dl-cdn.alpinelinux.org/alpine/v3.17/main".into(),
                                "--repository".into(),
                                "https://dl-cdn.alpinelinux.org/alpine/v3.17/community".into(),
                                // standard packages
                                "alpine-base".into(),
                                "linux-virt".into(),
                                // XXX "linux-edge".into(),
                                // XXX "linux-lts".into(),
                                // setup-alpine running with --quick doesn't install these!
                                "openssh".into(),
                                "chrony".into(),
                            ],
                            world_lines,
                        ]
                        .concat(),
                    )?;

                    // SUPER important to run this before setting runlevel stuff
                    // below. In particular, `killprocs` will nuke the host
                    // machine's processes!
                    drop(drop_command);

                    // whyyyyyyyyyyyy does something set acpid and crond to
                    // sysinit?!
                    run(
                        "chroot".into(),
                        &[
                            mount_partition_3.dest(),
                            "rc-update".into(),
                            "delete".into(),
                            "acpid".into(),
                            "sysinit".into(),
                        ],
                    )?;
                    run(
                        "chroot".into(),
                        &[
                            mount_partition_3.dest(),
                            "rc-update".into(),
                            "delete".into(),
                            "crond".into(),
                            "sysinit".into(),
                        ],
                    )?;

                    // https://superuser.com/questions/193115/root-file-system-is-mounted-read-only-on-boot-on-gentoo-linux
                    // install alpine, then look at `find /etc/runlevels/ | sort`
                    // also inspect `rc-update show -v | grep -v '| *$'`
                    // XXX shouldn't something do this already?
                    let runlevel_settings = [
                        // boot
                        ("bootmisc", "boot"),
                        ("hostname", "boot"),
                        ("hwclock", "boot"),
                        ("modules", "boot"),
                        ("networking", "boot"),
                        ("seedrng", "boot"),
                        ("swap", "boot"),
                        ("sysctl", "boot"),
                        ("syslog", "boot"),
                        //("netmount", "boot"),
                        //("procfs", "boot"),
                        //("termencoding", "boot"),
                        // sysinit
                        ("devfs", "sysinit"),
                        ("dmesg", "sysinit"),
                        ("hwdrivers", "sysinit"),
                        ("mdev", "sysinit"),
                        //("modloop", "sysinit"),
                        // default
                        //("root", "default"),
                        //("sysfs", "default"),
                        //("fsck", "default"),
                        //("localmount", "default"),
                        // services
                        ("acpid", "default"),
                        ("crond", "default"),
                        ("sshd", "default"),
                        ("chronyd", "default"),
                        // shutdown
                        ("killprocs", "shutdown"),
                        ("mount-ro", "shutdown"),
                        ("savecache", "shutdown"),
                    ];

                    for (service, runlevel) in runlevel_settings {
                        /*
                        run(
                            "chroot".into(),
                            &[
                                mount_partition_3.dest(),
                                "rc-service".into(),
                                service.into(),
                                "start".into(),
                                "--ifstopped".into(),
                                // suppress errors
                                "-q".into(),
                                "-q".into(),
                            ],
                        )?;
                        */
                        run(
                            "chroot".into(),
                            &[
                                mount_partition_3.dest(),
                                "rc-update".into(),
                                "add".into(),
                                service.into(),
                                runlevel.into(),
                            ],
                        )?;
                    }

                    // XXX setup-user
                    // XXX setup-cloud-init, run at boot
                }
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

            writeln!(
                fstab,
                "{} / ext4 defaults,errors=remount-ro 0 1",
                p3_fs_uuid
            )?;

            let p2_fs_uuid: String = output_stdout_string(&run(
                "blkid".into(),
                &["-o".into(), "export".into(), root_device_partition_2],
            )?)
            .split('\n')
            .filter(|x| x.starts_with("UUID="))
            .collect();

            writeln!(fstab, "{} /boot/efi vfat defaults 0 2", p2_fs_uuid)?;

            if matches!(flavor, OsFlavor::Alpine) {
                writeln!(fstab, "tmpfs /tmp tmpfs nosuid,nodev 0 0")?;

                // use partition 4 as swap
                let swap_partition = format!("{}{}", partitioned_disk.path(), "p4");

                run("mkswap".into(), &[swap_partition.clone()])?;

                let swap_uuid: String = output_stdout_string(&run(
                    "blkid".into(),
                    &["-o".into(), "export".into(), swap_partition],
                )?)
                .split('\n')
                .filter(|x| x.starts_with("UUID="))
                .collect();

                writeln!(fstab, "{} swap swap defaults 0 0", swap_uuid)?;
            }

            drop(fstab);

            run(
                "cat".into(),
                &[format!("{}/etc/fstab", mount_partition_3.dest())],
            )?;

            println!("> write hosts");

            // XXX hostname parameter
            let mut hosts = File::create(format!("{}/etc/hosts", mount_partition_3.dest()))?;
            writeln!(
                hosts,
                r#"
127.0.0.1	localhost localhost.localdomain
127.0.1.1	alpine

# The following lines are desirable for IPv6 capable hosts
::1     ip6-localhost ip6-loopback
fe00::0 ip6-localnet
ff00::0 ip6-mcastprefix
ff02::1 ip6-allnodes
ff02::2 ip6-allrouters
                "#,
            )?;

            drop(hosts);

            println!("> write hostname");

            let mut hostname = File::create(format!("{}/etc/hostname", mount_partition_3.dest()))?;
            writeln!(hostname, "alpine")?;
            drop(hostname);

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
            writeln!(device_map, "(hd0) {}", partitioned_disk.path())?;
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
            writeln!(grub_file, "GRUB_TERMINAL=\"serial console\"")?;
            writeln!(
                grub_file,
                "{}",
                match flavor {
                    OsFlavor::Debian | OsFlavor::Ubuntu =>
                        "GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash console=ttyS0,115200 init=/lib/systemd/systemd-bootchart\"",

                    OsFlavor::Alpine =>
                        "GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash console=tty0 console=ttyS0,115200 rootfstype=ext4 modules=sd-mod,usb-storage,nvme,ext4\"",
                }
            )?;
            drop(grub_file);

            run(
                "grub-install".into(),
                &[
                    "--target=x86_64-efi".into(),
                    format!("--efi-directory={}/boot/efi/", mount_partition_3.dest()),
                    format!("--root-directory={}", mount_partition_3.dest()),
                    "--no-floppy".into(),
                    partitioned_disk.path(),
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

            match flavor {
                OsFlavor::Debian | OsFlavor::Ubuntu => {
                    println!("> update-initramfs");
                    run(
                        "chroot".into(),
                        &[
                            mount_partition_3.dest(),
                            "update-initramfs".into(),
                            "-u".into(),
                        ],
                    )?;
                }

                OsFlavor::Alpine => {
                    // by default, mkinitfs will use the docker host's kernel version
                    println!("> get kernel version");

                    let mut kernelversion: Vec<String> =
                        std::fs::read_dir(format!("{}/lib/modules/", mount_partition_3.dest()))?
                            .collect::<Result<Vec<std::fs::DirEntry>, std::io::Error>>()?
                            .into_iter()
                            .map(|x| {
                                let full_path = x.path();
                                let last_part = full_path.file_name().unwrap();
                                last_part.to_os_string().into_string().unwrap()
                            })
                            .collect();

                    println!("detected kernel versions {:?}", kernelversion);
                    if kernelversion.len() != 1 {
                        bail!("incorrect number of kernel vers");
                    }

                    let kernelversion: String = kernelversion.pop().unwrap();

                    println!("> mkinitfs");
                    run(
                        "chroot".into(),
                        &[
                            mount_partition_3.dest(),
                            "mkinitfs".into(),
                            "-c".into(),
                            "/etc/mkinitfs/mkinitfs.conf".into(),
                            "-b".into(),
                            "/".into(),
                            kernelversion,
                        ],
                    )?;
                }
            }

            if matches!(flavor, OsFlavor::Alpine) {
                // alpine requires changing /etc/inittab for a login console on
                // ttyS0
                run(
                    "chroot".into(),
                    &[
                        mount_partition_3.dest(),
                        "sed".into(),
                        "-i".into(),
                        "-e".into(),
                        "s/^#ttyS0/ttyS0/g".into(),
                        "/etc/inittab".into(),
                    ],
                )?;

                // alpine also tries to run sysinit and boot in parallel?!
                /*
                run(
                    "chroot".into(),
                    &[
                        mount_partition_3.dest(),
                        "sed".into(),
                        "-i".into(),
                        "-e".into(),
                        "s_::sysinit:/sbin/openrc boot_::wait:/sbin/openrc boot_g".into(),
                        "/etc/inittab".into(),
                    ],
                )?;
                */
            }

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

            println!(
                "> Copy {:?} to {:?}",
                partitioned_disk.img_path(),
                output_file
            );
            std::fs::copy(partitioned_disk.img_path(), output_file)?;
        }
    }

    Ok(())
}
