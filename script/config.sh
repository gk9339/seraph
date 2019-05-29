#!/bin/sh
set -e

SYSTEM_HEADER_PROJECTS="libc kernel bin"
PROJECTS="libc kernel bin"

export MAKE=${MAKE:-make}
export HOST=${HOST:-$(./script/default-host.sh)}
export TOOLCHAIN="$(pwd)/toolchain/bin/"

export AR=${TOOLCHAIN}${HOST}-ar
export AS=${TOOLCHAIN}${HOST}-as
export CC=${TOOLCHAIN}${HOST}-gcc
export NM=${TOOLCHAIN}${HOST}-nm

export PREFIX=/
export EXEC_PREFIX=$PREFIX
export BOOTDIR=/boot
export LIBDIR=$EXEC_PREFIX/lib
export INCLUDEDIR=$PREFIX/include

export CFLAGS='-O0 -g'

# Configure the cross-compiler to use the desired system root.
export SYSROOT="$(pwd)/sysroot"
export CC="$CC --sysroot=$SYSROOT"

# Work around that the -elf gcc targets doesn't have a system include directory
# because it was configured with --without-headers rather than --with-sysroot.
if echo "$HOST" | grep -Eq -- '-elf($|-)'; then
  export CC="$CC -isystem=$INCLUDEDIR"
fi
