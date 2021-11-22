#!/bin/bash

make

docker build -t crucible-tester -f Dockerfile.crucible_tester .

BYTES=$(docker inspect crucible-tester | jq .[0].Size)
GBS=$(python3 -c "import math; print(1 + int(math.ceil((float(${BYTES}) / (1024.0**3)))));")

echo "docker image is ${BYTES} b -> bootable image is ${GBS} Gb"

sudo \
    ./target/debug/docker_to_uefi_bootable_image \
    create \
        --image-name crucible-tester \
        --output-file crucible-tester.img \
        --disk-size ${GBS} \
        --root-passwd crucible \

pigz -c crucible-tester.img > crucible-tester.img.gz

ls -alh crucible-tester.img.gz

