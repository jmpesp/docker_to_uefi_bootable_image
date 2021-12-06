Take what's in a Docker image and make it into a bootable UEFI disk image.

An example:

    sudo \
        ./target/debug/docker_to_uefi_bootable_image \
        create \
            --image-name debian:latest \
            --output-file debian.img \
            --disk-size 2 \
            --root-passwd nNGQlzZxBYxBmPIgpEP5ezgbqPb4L2R4 \
            --flavor debian

Test with QEMU:

    sudo qemu-system-x86_64 \
        -m 2g \
        -bios /usr/share/OVMF/OVMF_CODE.fd \
        -drive file=debian.img,if=virtio,format=raw

Mongo:

    sudo \
        ./target/debug/docker_to_uefi_bootable_image \
            create \
                --image-name mongo:4 \
                --output-file mongo.img \
                --disk-size 8 \
                --root-passwd mongo \
                --flavor ubuntu

Note that this will currently not give you an image that boots and runs Mongo,
there's a bunch of manual work that's required, but the image will contain all
the installed software.

Only tested with Xubuntu.


