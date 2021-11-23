#!/bin/bash
set -eu

make

sudo \
    ./target/debug/docker_to_uefi_bootable_image \
        create \
            --image-name mongo:4 \
            --output-file mongo.img \
            --disk-size 8 \
            --root-passwd mongo \
            --flavor ubuntu

rm mongo-sparse.img mongo-sparse.img.gz || true

virt-sparsify mongo.img mongo-sparse.img

pigz mongo-sparse.img

ls -alh mongo-sparse.img.gz
