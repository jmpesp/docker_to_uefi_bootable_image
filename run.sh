#!/bin/bash
set -e

make

docker build -t debian-tester -f Dockerfile.debian-tester --no-cache .

sudo \
    ./target/debug/docker_to_uefi_bootable_image \
    create \
        --image-name debian-tester \
        --output-file debian.img \
        --disk-size 2 \
        --root-passwd nNGQlzZxBYxBmPIgpEP5ezgbqPb4L2R4 \
        --flavor debian

