#!/bin/sh
set -e

SYSTEM_HEADER_PROJECTS="libc kernel linker sbin bin"
PROJECTS="libc kernel linker sbin bin"

export SYSROOT="$(pwd)/sysroot"
export TOOLCHAIN="$(pwd)/toolchain/bin/"

export MAKE=${MAKE:-make}
export HOST=${HOST:-$(./script/default-host.sh)}
export PATH=${TOOLCHAIN}:$PATH

export AR=${HOST}-ar
export AS=${HOST}-as
export CC=${HOST}-gcc
export NM=${HOST}-nm

export PREFIX=/
export EXEC_PREFIX=$PREFIX
export BOOTDIR=/boot
export LIBDIR=$EXEC_PREFIX/lib
export INCLUDEDIR=$PREFIX/include

export CFLAGS='-g'
export CPPFLAGS=''

# Work around that the -elf gcc targets doesn't have a system include directory
# because it was configured with --without-headers rather than --with-sysroot.
if echo "$HOST" | grep -Eq -- '-elf($|-)'; then
  export CC="$CC -isystem=$INCLUDEDIR"
fi
