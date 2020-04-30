#!/bin/bash
set -e

. ./script/config.sh

if [ ! $(command -v i686-seraph-gcc) ]; then
    . ./toolchain.sh
else
    . ./headers.sh
fi

for PROJECT in $PROJECTS; do
  (cd $PROJECT && DESTDIR="$SYSROOT" $MAKE -j$NUMCPU install)
done
