#!/bin/bash
set -eu

make

docker build -t mongo-tester -f Dockerfile.mongo --no-cache .

sudo \
    ./target/debug/docker_to_uefi_bootable_image \
        create \
            --image-name mongo-tester \
            --output-file mongo.img \
            --disk-size 8 \
            --root-passwd mongo \
            --flavor ubuntu

rm mongo-sparse.img mongo-sparse.img.gz || true

sudo virt-sparsify mongo.img mongo-sparse.img

sudo chown ${USER} mongo.img mongo-sparse.img

pigz mongo-sparse.img

ls -alh mongo-sparse.img.gz
