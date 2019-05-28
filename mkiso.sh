#!/bin/sh
set -e
. ./build.sh

mkdir -p isodir
mkdir -p isodir/boot
mkdir -p isodir/boot/grub

./initrd.py

cp sysroot/boot/seraph.kernel isodir/boot/seraph.kernel
cat > isodir/boot/grub/grub.cfg << EOF
set timeout=0
set default=0

menuentry "seraph" {
	multiboot /boot/seraph.kernel root=/dev/ram0 root_type=tar
    module /boot/seraph.initrd
}
EOF

grub-mkrescue -o seraph.iso isodir
