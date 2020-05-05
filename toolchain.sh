#!/bin/bash
set -e
. ./headers.sh

echo -e "    \033[1m\033[38;5;14m:: Building toolchain\033[0m"

echo "UNSET HOST AR AS CC CXX NM PREFIX CFLAGS CXXFLAGS"
unset HOST
unset AR
unset AS
unset CC
unset CXX
unset NM
unset PREFIX
unset CFLAGS
unset CXXFLAGS

TARGET=i686-seraph
echo "TARGET=\"$TARGET\""
DIR=$(pwd)
echo "DIR=\"$DIR\""
PREFIX=$DIR/toolchain
echo "PREFIX=\"$PREFIX\""

export CFLAGS='-O2'
echo "CFLAGS=\"$CFLAGS\""
export CXXFLAGS='-O2'
echo "CXXFLAGS=\"$CXXFLAGS\""

echo "MKDIR $PREFIX"
mkdir -p "$PREFIX"
echo "CD $PREFIX"
cd "$PREFIX"

echo "MKDIR tarballs"
mkdir -p tarballs

echo "PUSHD tarballs"
pushd tarballs > /dev/null
    if [ ! -e "binutils-2.32.tar.gz" ]; then
        echo "CURL binutils-2.32.tar.gz"
        curl "https://ftp.gnu.org/gnu/binutils/binutils-2.32.tar.gz" -o binutils-2.32.tar.gz
    fi
    if [ ! -e "gcc-8.3.0.tar.gz" ]; then
        echo "CURL gcc-8.3.0.tar.gz"
        curl "https://ftp.gnu.org/gnu/gcc/gcc-8.3.0/gcc-8.3.0.tar.gz" -o gcc-8.3.0.tar.gz
    fi

    if [ ! -d "binutils-2.32" ]; then
        echo "UNTAR binutils-2.32.tar.gz"
        tar -xf "binutils-2.32.tar.gz"
        echo "PUSHD binutils-2.32"
        pushd "binutils-2.32" > /dev/null
            echo "PATCH binutils.patch"
            patch -p1 < $PREFIX/patches/binutils.patch &> /dev/null
            echo "PUSHD ld"
            pushd "ld" > /dev/null
                echo "AUTOMAKE-1.15"
                automake-1.15 &> /dev/null
            echo "POPD binutils-2.32"
            popd > /dev/null
        echo "POPD tarballs"
        popd > /dev/null
    fi

    if [ ! -d "gcc-8.3.0" ]; then
        echo "UNTAR gcc-8.3.0.tar.gz"
        tar -xf "gcc-8.3.0.tar.gz"
        echo "PUSHD gcc-8.3.0"
        pushd "gcc-8.3.0" > /dev/null
            echo "PATCH gcc.patch"
            patch -p1 < $PREFIX/patches/gcc.patch &> /dev/null
            echo "AUTOCONF"
            autoconf &> /dev/null
            echo "PUSHD gcc"
            pushd "gcc" > /dev/null
                echo "AUTOCONF"
                autoconf &> /dev/null
            echo "POPD gcc-8.3.0"
            popd > /dev/null
            echo "PUSHD libgcc"
            pushd "libgcc" > /dev/null
                echo "AUTOCONF"
                autoconf &> /dev/null
            echo "POPD gcc-8.3.0"
            popd > /dev/null
            echo "PUSHD libstdc++-v3"
            pushd "libstdc++-v3" > /dev/null
                echo "AUTOCONF"
                autoconf &> /dev/null
            echo "POPD gcc-8.3.0"
            popd > /dev/null
        echo "POPD tarballs"
        popd > /dev/null
    fi
echo "POPD toolchain"
popd > /dev/null

echo "MKDIR binutils-build"
mkdir -p binutils-build
echo "MKDIR gcc-build"
mkdir -p gcc-build

echo "PUSHD binutils-build"
pushd binutils-build > /dev/null
    echo -e " \033[38;5;3m=> CONFIGURE binutils-2.32\033[0m"
    $DIR/toolchain/tarballs/binutils-2.32/configure --target=$TARGET --prefix=$PREFIX --with-sysroot=$SYSROOT --disable-werror &> /dev/null || exit 1
    echo -e " \033[38;5;3m=> MAKE all-binutils all-gas all-ld\033[0m"
    make -j$NUMCPU all-binutils all-gas all-ld &> /dev/null
    echo -e " \033[38;5;3m=> MAKE install-binutils install-gas install-ld\033[0m"
    make install-binutils install-gas install-ld &> /dev/null
echo "POPD toolchain"
popd > /dev/null

echo "PUSHD gcc-build"
pushd gcc-build > /dev/null
    echo -e " \033[38;5;3m=> CONFIGURE gcc-8.3.0\033[0m"
    $DIR/toolchain/tarballs/gcc-8.3.0/configure --target=$TARGET --prefix=$PREFIX --with-sysroot=$SYSROOT --disable-nls --disable-libstdcxx-pch --disable-multilib --disable-shared --enable-languages=c,c++ &> /dev/null || exit 1
    echo -e " \033[38;5;3m=> MAKE all-gcc all-target-libgcc\033[0m"
    make -j$NUMCPU inhibit_libc=true all-gcc all-target-libgcc &> /dev/null
    echo -e " \033[38;5;3m=> MAKE install-gcc install-target-libgcc\033[0m"
    make install-gcc install-target-libgcc &> /dev/null
echo "POPD toolchain"
popd > /dev/null

echo "CD $DIR"
cd "$DIR"

. ./headers.sh
echo -e " \033[38;5;3m=> MAKE libc\033[0m"
cd libc && DESTDIR="$SYSROOT" $MAKE -j$NUMCPU install

echo "CD $DIR"
cd "$DIR"

echo -e "    \033[1m\033[38;5;14m:: Building toolchain phase 2\033[0m"

echo "UNSET HOST AR AS CC CXX NM PREFIX CFLAGS CXXFLAGS"
unset HOST
unset AR
unset AS
unset CC
unset CXX
unset NM
unset PREFIX
unset CFLAGS
unset CXXFLAGS

TARGET=i686-seraph
echo "TARGET=\"$TARGET\""
DIR=$(pwd)
echo "DIR=\"$DIR\""
PREFIX=$DIR/toolchain
echo "PREFIX=\"$PREFIX\""

export CFLAGS='-O2'
echo "CFLAGS=\"$CFLAGS\""
export CXXFLAGS='-O2'
echo "CXXFLAGS=\"$CXXFLAGS\""

echo "CD $PREFIX"
cd "$PREFIX"

echo "MKDIR gcc-build-2"
mkdir -p gcc-build-2
echo "PUSHD gcc-build-2"
pushd gcc-build-2 > /dev/null
    echo -e " \033[38;5;3m=> CONFIGURE gcc-8.3.0\033[0m"
    echo "CONFIGURE gcc-8.3.0"
    $DIR/toolchain/tarballs/gcc-8.3.0/configure --target=$TARGET --prefix=$PREFIX --with-sysroot=$SYSROOT --disable-nls --disable-libstdcxx-pch --disable-multilib --enable-languages=c,c++ &> /dev/null || exit 1
    echo -e " \033[38;5;3m=> MAKE all-gcc all-target-libgcc all-target-libstdc++-v3\033[0m"
    make -j$NUMCPU all-gcc all-target-libgcc all-target-libstdc++-v3 &> /dev/null
    echo -e " \033[38;5;3m=> MAKE install-gcc install-target-libgcc install-target-libstdc++-v3\033[0m"
    make install-gcc install-target-libgcc install-target-libstdc++-v3 &> /dev/null
echo "POPD toolchain"
popd > /dev/null

echo "CD $DIR"
cd "$DIR"

. ./script/config.sh
echo -e " \033[38;5;3m=> MAKE libc clean\033[0m"
cd libc && $MAKE clean
echo "CD $DIR"
cd "$DIR"
echo "RM sysroot"
rm -rf sysroot
