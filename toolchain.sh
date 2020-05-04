#!/bin/bash
set -e
. ./headers.sh

echo -e "    \033[1m\033[38;5;14m:: Building toolchain\033[0m"

echo "UNSET MAKE HOST AR AS CC CXX NM PREFIX CFLAGS CXXFLAGS"
unset MAKE
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
pushd tarballs
    if [ ! -e "binutils-2.32.tar.gz" ]; then
        echo "CURL binutils-2.32.tar.gz"
        curl "https://ftp.gnu.org/gnu/binutils/binutils-2.32.tar.gz"
    fi
    if [ ! -e "gcc-8.3.0.tar.gz" ]; then
        echo "CURL gcc-8.3.0.tar.gz"
        curl "https://ftp.gnu.org/gnu/gcc/gcc-8.3.0/gcc-8.3.0.tar.gz"
    fi

    if [ ! -d "binutils-2.32" ]; then
        echo "UNTAR binutils-2.32.tar.gz"
        tar -xf "binutils-2.32.tar.gz"
        echo "PUSHD binutils-2.32"
        pushd "binutils-2.32"
            echo "PATCH binutils.patch"
            patch -p1 < $PREFIX/patches/binutils.patch
            echo "PUSHD ld"
            pushd "ld"
                echo "AUTOMAKE-1.15"
                automake-1.15 &> /dev/null
            echo "POPD"
            popd
        echo "POPD"
        popd
    fi

    if [ ! -d "gcc-8.3.0" ]; then
        echo "UNTAR gcc-8.3.0.tar.gz"
        tar -xf "gcc-8.3.0.tar.gz"
        echo "PUSHD gcc-8.3.0"
        pushd "gcc-8.3.0"
            echo "PATCH gcc.patch"
            patch -p1 < $PREFIX/patches/gcc.patch
            echo "AUTOCONF"
            autoconf &> /dev/null
            echo "PUSHD gcc"
            pushd "gcc"
                echo "AUTOCONF"
                autoconf &> /dev/null
            echo "POPD"
            popd
            echo "PUSHD libgcc"
            pushd "libgcc"
                echo "AUTOCONF"
                autoconf &> /dev/null
            echo "POPD"
            popd
            echo "PUSHD libstdc++-v3"
            pushd "libstdc++-v3"
                echo "AUTOCONF"
                autoconf &> /dev/null
            echo "POPD"
            popd
        echo "POPD"
        popd
    fi
echo "POPD"
popd

echo "MKDIR binutils-build"
mkdir -p binutils-build
echo "MKDIR gcc-build"
mkdir -p gcc-build

echo "PUSHD binutils-build"
pushd binutils-build
    echo "CONFIGURE binutils-2.32"
    $DIR/toolchain/tarballs/binutils-2.32/configure --target=$TARGET --prefix=$PREFIX --with-sysroot=$SYSROOT --disable-werror &> /dev/null || exit 1
    echo "MAKE all-binutils all-gas all-ld"
    make -j$NUMCPU all-binutils all-gas all-ld &> /dev/null
    echo "MAKE install-binutils install-gas install-ld"
    make install-binutils install-gas install-ld &> /dev/null
echo "POPD"
popd

echo "PUSHD gcc-build"
pushd gcc-build
    echo "CONFIGURE gcc-8.3.0"
    $DIR/toolchain/tarballs/gcc-8.3.0/configure --target=$TARGET --prefix=$PREFIX --with-sysroot=$SYSROOT --disable-nls --disable-libstdcxx-pch --disable-multilib --disable-shared --enable-languages=c,c++ &> /dev/null || exit 1
    echo "MAKE all-gcc all-target-libgcc"
    make -j$NUMCPU inhibit_libc=true all-gcc all-target-libgcc &> /dev/null
    echo "MAKE install-gcc install-target-libgcc"
    make install-gcc install-target-libgcc &> /dev/null
echo "POPD"
popd

echo "CD $DIR"
cd "$DIR"

. ./script/config.sh
echo -e " \033[38;5;3m=> MAKE libc\033[0m"
cd libc && DESTDIR="$SYSROOT" $MAKE -j$NUMCPU install

echo "CD $DIR"
cd "$DIR"

echo -e "    \033[1m\033[38;5;14m:: Building toolchain phase 2\033[0m"

echo "UNSET MAKE HOST AR AS CC CXX NM PREFIX CFLAGS CXXFLAGS"
unset MAKE
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
pushd gcc-build-2
    echo "CONFIGURE gcc-8.3.0"
    $DIR/toolchain/tarballs/gcc-8.3.0/configure --target=$TARGET --prefix=$PREFIX --with-sysroot=$SYSROOT --disable-nls --disable-libstdcxx-pch --disable-multilib --enable-languages=c,c++ || exit 1
    echo "MAKE all-gcc all-target-libgcc all-target-libstdc++-v3"
    make -j$NUMCPU all-gcc all-target-gcc all-target-libstdc++-v3
    echo "MAKE install-gcc install-target-libgcc install-target-libstdc++-v3"
    make install-gcc install-target-libgcc install-target-libstdc++-v3
echo "POPD"
popd

echo "CD $DIR"
cd "$DIR"

. ./script/config.sh
echo -e " \033[38;5;3m=> MAKE libc clean\033[0m"
cd libc && $MAKE clean
echo "CD $DIR"
cd "$DIR"
echo "RM sysroot"
rm -rf sysroot
