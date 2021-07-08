#!/bin/bash
set -e
. ./build.sh

echo -e "    \033[1m\033[38;5;14m:: Generating iso image\033[0m"

echo "MKDIR isodir/boot/grub"
mkdir -p isodir/boot/grub
echo "MKDIR $SYSROOT/boot/grub"
mkdir -p $SYSROOT/boot/grub
echo "MKDIR $SYSROOT/conf"
mkdir -p $SYSROOT/conf
echo "MKDIR $SYSROOT/home"
mkdir -p $SYSROOT/home
echo "MKDIR $SYSROOT/dev"
mkdir -p $SYSROOT/dev
echo "MKDIR $SYSROOT/proc"
mkdir -p $SYSROOT/proc
echo "MKDIR $SYSROOT/tmp"
mkdir -p $SYSROOT/tmp
echo "MKDIR $SYSROOT/src/conf"
mkdir -p $SYSROOT/src/conf

echo "CP conf/*"
cp -r conf/* $SYSROOT/conf

echo "CP home/*"
cp -r home/* $SYSROOT/home

echo "CP src/bin"
find bin \( -name '*.c' -o -name '*.cpp' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} $SYSROOT/src/ \;
echo "CP src/kernel"
find kernel \( -name '*.c' -o -name '*.cpp' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} $SYSROOT/src/ \;
echo "CP src/lib"
find lib \( -name '*.c' -o -name '*.cpp' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} $SYSROOT/src/ \;
echo "CP src/lib"
find libc \( -name '*.c' -o -name '*.cpp' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} $SYSROOT/src/ \;
echo "CP src/linker"
find linker \( -name '*.c' -o -name '*.cpp' -o -name '*.h' -o -name '*.S' -o -name '*.py' -o -name '*.ld' -o -name 'Makefile' -o -name 'make.config' \) -exec cp --parents {} $SYSROOT/src/ \;
echo "CP src/script"
find script \( -name '*.py' -o -name '*.sh' \) -exec cp --parents {} $SYSROOT/src/ \;
echo "CP toolchain/patches/*"
cp --parents toolchain/patches/*.patch $SYSROOT/src
echo "CP *sh Dockerfile .dockerignore LICENSE README.md "
cp *.sh Dockerfile .dockerignore LICENSE README.md $SYSROOT/src
echo "CP conf/*"
cp -r conf/* $SYSROOT/src/conf

echo "CAT > boot/grub/grub.cfg"
cat > $SYSROOT/boot/grub/grub.cfg << EOF
set timeout=1
set default=0

menuentry "seraph" {
    multiboot /boot/seraph.kernel root=/dev/ram0 root_type=ustar
    module /boot/seraph.initrd
}

menuentry "seraph - splash/serial" {
    multiboot /boot/seraph.kernel serialdebug root=/dev/ram0 root_type=ustar splash
    module /boot/seraph.initrd
}

menuentry "seraph - splash/serial - 720" {
    multiboot /boot/seraph.kernel serialdebug root=/dev/ram0 root_type=ustar splash
    set gfxpayload=1280x720x32,1024x768x32,800x600x32
    module /boot/seraph.initrd
}

menuentry "seraph - splash/serial - 1080" {
    multiboot /boot/seraph.kernel serialdebug root=/dev/ram0 root_type=ustar splash
    set gfxpayload=1920x1080x32,1280x1024x32,1024x768x32,800x600x32
    module /boot/seraph.initrd
}
EOF

echo "PYTHON3 seraph.initrd"
./script/initrd.py
echo "CP boot/grub/grub.cfg"
cp $SYSROOT/boot/grub/grub.cfg isodir/boot/grub/grub.cfg
echo "CP boot/seraph.kernel"
cp $SYSROOT/boot/seraph.kernel isodir/boot/seraph.kernel
echo "CP boot/seraph.initrd"
cp $SYSROOT/boot/seraph.initrd isodir/boot/seraph.initrd

echo "GRUB-MKRESCUE seraph.iso"
grub-mkrescue -o seraph.iso isodir &> /dev/null
