#!/usr/bin/python3

import tarfile

with tarfile.open('seraph.initrd', 'w') as ramdisk:
    ramdisk.add('sysroot', arcname='/')
