#!/bin/bash
set -eu
sudo qemu-system-x86_64 \
    -m 2g \
    -bios /usr/share/OVMF/OVMF_CODE.fd \
    -drive file=${1},if=virtio,format=raw

