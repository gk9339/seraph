#!/bin/bash
set -e

. ../script/config.sh

export CFLAGS="-Os -g"
export CXXFLAGS="-Os -g"

mkdir -p binutils-port
pushd binutils-port
    $TOOLCHAIN/tarballs/binutils-2.32/configure --host=$HOST --with-sysroot=/ --libexecdir=/lib --exec-prefix='' --disable-nls --disable-werror || exit 1
    make all-binutils all-gas all-ld
    make DESTDIR=$SYSROOT install-binutils install-gas install-ld
popd

mkdir -p gmp-port
pushd gmp-build
    $TOOLCHAIN/tarballs/gmp-6.2.0/configure --host=$HOST --prefix=/ || exit 1
    make
    make DESTDIR=$SYSROOT install
popd

mkdir -p mpfr-port
pushd mpfr-build
    $TOOLCHAIN/tarballs/mpfr-4.0.2/configure --host=$HOST --prefix=/ --with-sysroot=$SYSROOT || exit 1
    make
    make DESTDIR=$SYSROOT install
popd

mkdir -p mpc-port
pushd mpc-build
    $TOOLCHAIN/tarballs/mpc-1.1.0/configure --host=$HOST --prefix=/ --with-sysroot=$SYSROOT || exit 1
    make
    make DESTDIR=$SYSROOT install
popd

mkdir -p gcc-port
pushd gcc-port
    $TOOLCHAIN/tarballs/gcc-8.3.0/configure --host=$HOST --with-build-sysroot=$SYSROOT --prefix='' --libexecdir=/lib --disable-nls --disable-multilib --enable-initfini-array --enable-languages=c,c++ || exit 1
    make -j8 all-gcc
    make DESTDIR=$SYSROOT install-gcc
#    make -j8 all-target-libgcc
#    make DESTDIR=$SYSROOT install-target-libgcc
#    make -j8 all-target-libstdc++-v3
#    make DESTDIR=$SYSROOT install-target-libstdc++-v3
popd
