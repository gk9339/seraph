#!/bin/bash
set -e
. ./script/config.sh

echo -e "    \033[1m\033[38;5;14m:: Cleaning\033[0m"

for PROJECT in $PROJECTS; do
    echo -e " \033[38;5;3m=> MAKE $PROJECT clean\033[0m"
    (cd $PROJECT && $MAKE clean)
done

echo "RM sysroot"
rm -rf sysroot
echo "RM isodir"
rm -rf isodir
echo "RM seraph.iso"
rm -rf seraph.iso
