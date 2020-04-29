#!/bin/bash
set -e
. ./headers.sh

unset MAKE
unset HOST
unset AR
unset AS
unset CC
unset CXX
unset NM
unset PREFIX
unset CFLAGS

TARGET=i686-seraph
DIR=$(pwd)
PREFIX=$DIR/toolchain

export CFLAGS='-O2'
export CXXFLAGS='-O2'

mkdir -p "$PREFIX"
cd "$PREFIX"

mkdir -p tarballs

pushd tarballs
    if [ ! -e "binutils-2.32.tar.gz" ]; then
        wget "https://ftp.gnu.org/gnu/binutils/binutils-2.32.tar.gz"
    fi
    if [ ! -e "gcc-8.3.0.tar.gz" ]; then
        wget "https://ftp.gnu.org/gnu/gcc/gcc-8.3.0/gcc-8.3.0.tar.gz"
    fi

    if [ ! -d "binutils-2.32" ]; then
        tar -xf "binutils-2.32.tar.gz"
        pushd "binutils-2.32"
            patch -p1 < $PREFIX/patches/binutils.patch
            pushd "ld"
                automake-1.15 &> /dev/null
            popd
        popd
    fi

    if [ ! -d "gcc-8.3.0" ]; then
        tar -xf "gcc-8.3.0.tar.gz"
        pushd "gcc-8.3.0"
            patch -p1 < $PREFIX/patches/gcc.patch
            autoconf &> /dev/null
            pushd "gcc"
                autoconf &> /dev/null
            popd
            pushd "libgcc"
                autoconf &> /dev/null
            popd
            pushd "libstdc++-v3"
                autoconf &> /dev/null
            popd
        popd
    fi
popd

mkdir -p binutils-build
mkdir -p gcc-build

unset PKG_CONFIG_LIBDIR

pushd binutils-build
    $DIR/toolchain/tarballs/binutils-2.32/configure --target=$TARGET --prefix=$PREFIX --with-sysroot=$SYSROOT --disable-werror || exit 1
    make -j$NUMCPU all-binutils all-gas all-ld
    make install-binutils install-gas install-ld
popd

pushd gcc-build
    $DIR/toolchain/tarballs/gcc-8.3.0/configure --target=$TARGET --prefix=$PREFIX --with-sysroot=$SYSROOT --disable-nls --disable-libstdcxx-pch --disable-multilib --enable-initfini-array --disable-shared --enable-languages=c,c++ || exit 1
    make -j$NUMCPU inhibit_libc=true all-gcc
    make install-gcc
    make -j$NUMCPU inhibit_libc=true all-target-libgcc
    make install-target-libgcc
popd

cd "$DIR"

. ./script/config.sh
cd libc && DESTDIR="$SYSROOT" $MAKE -j$NUMCPU install

cd "$DIR"

unset MAKE
unset HOST
unset AR
unset AS
unset CC
unset CXX
unset NM
unset PREFIX
unset CFLAGS

TARGET=i686-seraph
DIR=$(pwd)
PREFIX=$DIR/toolchain

export CFLAGS=-O2
export CXXFLAGS=-O2

cd "$PREFIX"

mkdir -p gcc-build-2
pushd gcc-build-2
    $DIR/toolchain/tarballs/gcc-8.3.0/configure --target=$TARGET --prefix=$PREFIX --with-sysroot=$SYSROOT --disable-nls --disable-libstdcxx-pch --disable-multilib --enable-initfini-array --enable-languages=c,c++ || exit 1
    make -j$NUMCPU all-gcc
    make install-gcc
    make -j$NUMCPU all-target-libgcc
    make install-target-libgcc
    make -j$NUMCPU all-target-libstdc++-v3
    make install-target-libstdc++-v3
popd

cd "$DIR"

. ./script/config.sh
cd libc && $MAKE clean
rm -rf sysroot
