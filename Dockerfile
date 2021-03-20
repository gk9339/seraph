FROM archlinux

RUN pacman -Syu --noconfirm base-devel wget git grub libisoburn python mtools

COPY . /seraph

RUN cd /seraph && ./toolchain.sh && rm -rf bin isodir kernel lib libc linker ports script sysroot toolchain/tarballs toolchain/gcc-build toolchain/gcc-build-2 toolchain/binutils-build toolchain/patches headers.sh toolchain.sh

RUN pacman -R --noconfirm bison fakeroot file groff gcc binutils patch pkgconf sed sudo wget && pacman -Sc --noconfirm

ENV PATH="/seraph/toolchain/bin:${PATH}"
