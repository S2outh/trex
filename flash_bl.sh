#!/bin/sh
probe-rs erase --chip STM32H723VG
cargo flash --manifest-path ./bootloader/Cargo.toml --release --chip STM32H723VG --target thumbv7em-none-eabihf
