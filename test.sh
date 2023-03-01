#!/bin/bash
set -eu

if [[ ${#} -lt 2 ]];
then
    echo "<base name> <overlay name>"
    exit 1
fi

#rm nvme_file.qcow2 || true
#qemu-img create -f qcow2 nvme_file.qcow2 4G

BASE="${1}"
OVERLAY="${2}"

rm "${OVERLAY}" || true
qemu-img create -F raw -f qcow2 -b "${BASE}" "${OVERLAY}"

sudo qemu-system-x86_64 \
    -enable-kvm \
    -m 2g \
    -bios /usr/share/OVMF/OVMF_CODE.fd \
    -drive file="${OVERLAY}",if=none,format=qcow2,id=nvme1 \
    -device nvme,drive=nvme1,serial=nvme-1,addr=0x4

#    -drive file="${OVERLAY}",if=virtio #\
