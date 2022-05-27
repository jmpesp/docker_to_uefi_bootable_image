#!/bin/bash
set -eu

#rm nvme_file.qcow2 || true
#qemu-img create -f qcow2 nvme_file.qcow2 4G

sudo qemu-system-x86_64 \
    -m 2g \
    -bios /usr/share/OVMF/OVMF_CODE.fd \
    -drive file=${1},if=virtio,format=raw #\
#    -drive file=nvme_file.qcow2,if=none,format=qcow2,id=nvme1 \
#    -device nvme,drive=nvme1,serial=nvme-1,addr=0x4

