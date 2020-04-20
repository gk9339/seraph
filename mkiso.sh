#!/bin/bash
set -e
. ./build.sh

mkdir -p isodir
mkdir -p isodir/boot
mkdir -p isodir/boot/grub
mkdir -p sysroot/boot/grub
mkdir -p sysroot/dev
mkdir -p sysroot/proc
mkdir -p sysroot/src

find bin \( -name '*.c' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} sysroot/src/ \;
find kernel \( -name '*.c' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} sysroot/src/ \;
find lib \( -name '*.c' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} sysroot/src/ \;
find libc \( -name '*.c' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} sysroot/src/ \;
find linker \( -name '*.c' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} sysroot/src/ \;
find script \( -name '*.py' -o -name '*.sh' \) -exec cp --parents {} sysroot/src/ \;
cp --parents toolchain/patches/*.patch sysroot/src
cp *.sh sysroot/src
cp LICENSE README.md sysroot/src

cat > sysroot/boot/grub/grub.cfg << EOF
set timeout=0
set default=0

menuentry "seraph" {
    multiboot /boot/seraph.kernel serialdebug root=/dev/ram0 root_type=ustar
    module /boot/seraph.initrd
}
EOF

./script/initrd.py
cp sysroot/boot/grub/grub.cfg isodir/boot/grub/grub.cfg
cp sysroot/boot/seraph.kernel isodir/boot/seraph.kernel
cp sysroot/boot/seraph.initrd isodir/boot/seraph.initrd

grub-mkrescue -o seraph.iso isodir
