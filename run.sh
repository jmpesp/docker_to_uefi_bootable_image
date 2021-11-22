#!/bin/bash
set -e

make

sudo \
    ./target/debug/docker_to_uefi_bootable_image \
    create \
        --image-name debian:latest \
        --output-file debian.img \
        --disk-size 2 \
        --root-passwd nNGQlzZxBYxBmPIgpEP5ezgbqPb4L2R4

