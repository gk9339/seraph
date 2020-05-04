#!/bin/bash
set -e

echo -e "    \033[1m\033[38;5;14m:: Configuring environment\033[0m"
SYSTEM_HEADER_PROJECTS="libc kernel lib bin"
echo "SYSTEM_HEADER_PROJECTS=\"$SYSTEM_HEADER_PROJECTS\""
PROJECTS="libc kernel linker lib bin"
echo "PROJECTS=\"$PROJECTS\""

export SYSROOT="$(pwd)/sysroot"
echo "SYSROOT=\"$SYSROOT\""
export TOOLCHAIN="$(pwd)/toolchain/bin/"
echo "TOOLCHAIN=\"$TOOLCHAIN\""

export MAKE=${MAKE:-make}
echo "MAKE=\"$MAKE\""
export HOST=${HOST:-$(./script/default-host.sh)}
echo "HOST=\"$HOST\""
export PATH=${TOOLCHAIN}:$PATH
echo "PATH=\"$PATH\""

export AR=${HOST}-ar
echo "AR=\"$AR\""
export AS=${HOST}-as
echo "AS=\"$AS\""
export CC=${HOST}-gcc
echo "CC=\"$CC\""
export CXX=${HOST}-g++
echo "CXX=\"$CXX\""
export NM=${HOST}-nm
echo "NM=\"$NM\""

export PREFIX=/
echo "PREFIX=\"$PREFIX\""
export EXEC_PREFIX=$PREFIX
echo "EXEC_PREFIX=\"$EXEC_PREFIX\""
export BOOTDIR=/boot
echo "BOOTDIR=\"$BOOTDIR\""
export LIBDIR=$EXEC_PREFIX/lib
echo "LIBDIR=\"$LIBDIR\""
export INCLUDEDIR=$PREFIX/include
echo "INCLUDEDIR=\"$INCLUDEDIR\""

export CFLAGS='-pipe -g -mmmx -msse -msse2 -msse3 -fstack-protector-strong'
echo "CFLAGS=\"$CFLAGS\""
export CXXFLAGS='-pipe -g -mmmx -msse -msse2 -msse3 -fstack-protector-strong'
echo "CXXFLAGS=\"$CXXFLAGS\""
export CPPFLAGS=''
echo "CPPFLAGS=\"$CPPFLAGS\""

export NUMCPU=$(grep -c '^processor' /proc/cpuinfo)
echo "NUMCPU=\"$NUMCPU\""
