#!/bin/bash
set -e

SYSTEM_HEADER_PROJECTS="libc kernel linker lib bin"
PROJECTS="libc kernel linker lib bin"

export SYSROOT="$(pwd)/sysroot"
export TOOLCHAIN="$(pwd)/toolchain/bin/"

export MAKE=${MAKE:-make}
export HOST=${HOST:-$(./script/default-host.sh)}
export PATH=${TOOLCHAIN}:$PATH

export AR=${HOST}-ar
export AS=${HOST}-as
export CC=${HOST}-gcc
export CXX=${HOST}-g++
export NM=${HOST}-nm

export PREFIX=/
export EXEC_PREFIX=$PREFIX
export BOOTDIR=/boot
export LIBDIR=$EXEC_PREFIX/lib
export INCLUDEDIR=$PREFIX/include

export CFLAGS='-pipe -g -mmmx -msse -msse2 -msse3 -fstack-protector-strong'
export CPPFLAGS=''

export NUMCPU=$(grep -c '^processor' /proc/cpuinfo)
