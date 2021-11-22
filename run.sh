#!/bin/bash
make
sudo ./target/debug/docker_to_uefi_bootable_image create --image-name debian:latest --flavor debian --output-file debian.img --disk-size 16
