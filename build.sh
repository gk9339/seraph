#!/bin/sh
set -e

if [ ! -f "./toolchain/bin/i686-seraph-gcc" ]; then
    . ./toolchain.sh
    . ./script/config.sh
else
    . ./headers.sh
fi

for PROJECT in $PROJECTS; do
  (cd $PROJECT && DESTDIR="$SYSROOT" $MAKE install)
done
