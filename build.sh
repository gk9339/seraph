#!/bin/bash
set -e

if [ ! -f "./toolchain/bin/i686-seraph-gcc" ]; then
    . ./toolchain.sh
else
    . ./headers.sh
fi

echo -e "    \033[1m\033[38;5;14m:: Building\033[0m"

echo "MKDIR sysroot/lib"
mkdir -p sysroot/lib

echo "CP lib/libstdc++.so lib/libgcc_s.so.1"
cp toolchain/i686-seraph/lib/libstdc++.so toolchain/i686-seraph/lib/libgcc_s.so.1 sysroot/lib

echo "LN libgcc_.so -> libgcc_s.so.1"
cd sysroot/lib && ln -sf libgcc_s.so.1 libgcc_s.so && cd ../..

for PROJECT in $PROJECTS; do
    echo -e " \033[38;5;3m=> MAKE $PROJECT\033[0m"
    (cd $PROJECT && DESTDIR="$SYSROOT" $MAKE -j$NUMCPU install)
done
