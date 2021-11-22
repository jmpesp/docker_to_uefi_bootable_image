#!/bin/bash
cargo run -q -- create --image-name debian:latest --flavor debian --output-file debian.img --disk-size 16
