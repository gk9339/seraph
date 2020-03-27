#!/bin/bash
set -e
. ./build.sh

mkdir -p isodir
mkdir -p isodir/boot
mkdir -p isodir/boot/grub
mkdir -p sysroot/boot/grub

mkdir -p sysroot/dev
./script/initrd.py

cat > sysroot/boot/grub/grub.cfg << EOF
set timeout=0
set default=0

menuentry "seraph" {
    multiboot /boot/seraph.kernel root=/dev/ram0 root_type=ustar
    module /boot/seraph.initrd
}
EOF
cp sysroot/boot/seraph.kernel isodir/boot/seraph.kernel
cp sysroot/boot/seraph.initrd isodir/boot/seraph.initrd
cp sysroot/boot/grub/grub.cfg isodir/boot/grub/grub.cfg

grub-mkrescue -o seraph.iso isodir
