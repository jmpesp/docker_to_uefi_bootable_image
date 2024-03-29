#!/bin/bash
set -e

make build

docker build -t crucible-tester -f Dockerfile.crucible_tester --no-cache .

BYTES=$(docker inspect crucible-tester | jq .[0].Size)
GBS=$(python3 -c "import math; print(2 + int(math.ceil((float(${BYTES}) / (1024.0**3)))));")

echo "docker image is ${BYTES} b -> bootable image is ${GBS} Gb"

sudo \
    ./target/debug/docker_to_uefi_bootable_image \
    create \
        --image-name crucible-tester \
        --output-file crucible-tester.img \
        --disk-size ${GBS} \
        --root-passwd crucible \
        --flavor debian

rm crucible-tester-sparse.img crucible-tester-sparse.img.gz || true

sudo virt-sparsify crucible-tester.img crucible-tester-sparse.img

sudo chown ${USER} crucible-tester.img crucible-tester-sparse.img

ls -alh crucible-tester-sparse.img

#pigz crucible-tester-sparse.img
#ls -alh crucible-tester-sparse.img.gz

