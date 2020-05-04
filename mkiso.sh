#!/bin/bash
set -e
. ./build.sh

mkdir -p isodir
mkdir -p isodir/boot
mkdir -p isodir/boot/grub
mkdir -p sysroot/boot/grub
mkdir -p sysroot/conf
mkdir -p sysroot/dev
mkdir -p sysroot/proc
mkdir -p sysroot/tmp
mkdir -p sysroot/src

cp -r conf/* sysroot/conf

cp toolchain/i686-seraph/lib/libstdc++.so toolchain/i686-seraph/lib/libgcc_s.so.1 sysroot/lib
cd sysroot/lib && ln -sf libgcc_s.so.1 libgcc_s.so && cd ../..

find bin \( -name '*.c' -o -name '*.cpp' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} sysroot/src/ \;
find kernel \( -name '*.c' -o -name '*.cpp' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} sysroot/src/ \;
find lib \( -name '*.c' -o -name '*.cpp' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} sysroot/src/ \;
find libc \( -name '*.c' -o -name '*.cpp' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} sysroot/src/ \;
find linker \( -name '*.c' -o -name '*.cpp' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} sysroot/src/ \;
find script \( -name '*.py' -o -name '*.sh' \) -exec cp --parents {} sysroot/src/ \;
cp --parents toolchain/patches/*.patch sysroot/src
cp *.sh sysroot/src
cp LICENSE README.md sysroot/src

cat > sysroot/boot/grub/grub.cfg << EOF
set timeout=1
set default=0

menuentry "seraph" {
    multiboot /boot/seraph.kernel root=/dev/ram0 root_type=ustar
    module /boot/seraph.initrd
}

menuentry "seraph - splash/serial" {
    multiboot /boot/seraph.kernel serialdebug root=/dev/ram0 root_type=ustar splash
    #set gfxpayload=1920x1080x32
    module /boot/seraph.initrd
}

menuentry "seraph - splash/serial - 720" {
    multiboot /boot/seraph.kernel serialdebug root=/dev/ram0 root_type=ustar splash
    set gfxpayload=1280x720x32
    module /boot/seraph.initrd
}

menuentry "seraph - splash/serial - 1080" {
    multiboot /boot/seraph.kernel serialdebug root=/dev/ram0 root_type=ustar splash
    set gfxpayload=1920x1080x32
    module /boot/seraph.initrd
}
EOF

./script/initrd.py
cp sysroot/boot/grub/grub.cfg isodir/boot/grub/grub.cfg
cp sysroot/boot/seraph.kernel isodir/boot/seraph.kernel
cp sysroot/boot/seraph.initrd isodir/boot/seraph.initrd

grub-mkrescue -o seraph.iso isodir
