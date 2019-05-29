#!/bin/sh
set -e
. ./headers.sh

unset MAKE
unset HOST
unset AR
unset AS
unset CC
unset NM
unset PREFIX
unset CFLAGS
unset SYSROOT

DIR="$( pwd )"

TARGET=i686-seraph
PREFIX="$DIR/toolchain"
SYSROOT="$DIR/sysroot"

mkdir -p $PREFIX
cd "$PREFIX"

mkdir -p tarballs

pushd tarballs
    if [ ! -e "binutils-2.32.tar.gz" ]; then
        wget "http://ftp.gnu.org/gnu/binutils/binutils-2.32.tar.gz"
    fi
    if [ ! -e "gcc-8.3.0.tar.gz" ]; then
        wget "https://ftp.gnu.org/gnu/gcc/gcc-8.3.0/gcc-8.3.0.tar.gz"
    fi

    if [ ! -d "binutils-2.32" ]; then
        tar -xf "binutils-2.32.tar.gz"
        pushd "binutils-2.32"
            patch -p1 < $DIR/toolchain-patches/binutils.patch
            pushd "ld"
                automake-1.15
            popd
        popd
    fi

    if [ ! -d "gcc-8.3.0" ]; then
        tar -xf "gcc-8.3.0.tar.gz"
        pushd "gcc-8.3.0"
            patch -p1 < $DIR/toolchain-patches/gcc.patch
            pushd "libstdc++-v3"
                automake-1.11
            popd
        popd
    fi
popd

mkdir -p local
mkdir -p binutils-build
mkdir -p gcc-build

unset PKG_CONFIG_LIBDIR
set -x
pushd binutils-build
    $DIR/toolchain/tarballs/binutils-2.32/configure --target=$TARGET --prefix=$PREFIX --with-sysroot=$SYSROOT --disable-werror || exit 1
    make -j4
    make install
popd

pushd gcc-build
    $DIR/toolchain/tarballs/gcc-8.3.0/configure --target=$TARGET --prefix=$PREFIX --with-sysroot=$SYSROOT --disable-nls --enable-languages=c || exit 1
    make all-gcc all-target-libgcc
    make install-gcc install-target-libgcc
popd
