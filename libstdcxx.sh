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

cd "$PREFIX"

pushd gcc-build
    if [ ! -e ${PREFIX}/${TARGET}/lib/libstdc++.a ]; then
        make all-target-libstdc++-v3
        make install-target-libstdc++-v3
    fi
popd

cd "$DIR"
