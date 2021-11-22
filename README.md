Take what's in a Docker image and make it into a bootable UEFI disk image.

An example:

    sudo \
        ./target/debug/docker_to_uefi_bootable_image \
        create \
            --image-name debian:latest \
            --output-file debian.img \
            --disk-size 2 \
            --root-passwd nNGQlzZxBYxBmPIgpEP5ezgbqPb4L2R4

Test with QEMU:

    sudo qemu-system-x86_64 \
        -m 2g \
        -bios /usr/share/OVMF/OVMF_CODE.fd \
        -drive file=debian.img,if=virtio,format=raw

