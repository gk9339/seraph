#!/bin/bash
set -e
. ./script/config.sh

echo -e "    \033[1m\033[38;5;14m:: Installing headers\033[0m"

echo "MKDIR $SYSROOT"
mkdir -p "$SYSROOT"

for PROJECT in $SYSTEM_HEADER_PROJECTS; do
    echo -e " \033[38;5;3m=> MAKE $PROJECT headers\033[0m"
    (cd $PROJECT && DESTDIR="$SYSROOT" $MAKE install-headers)
done
