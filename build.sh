#!/bin/sh
set -e
. ./headers.sh

if [ ! -f "./toolchain/bin/i686-seraph-gcc" ]; then
    . ./toolchain.sh
fi

for PROJECT in $PROJECTS; do
  (cd $PROJECT && DESTDIR="$SYSROOT" $MAKE install)
done
