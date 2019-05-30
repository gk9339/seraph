# seraph

# Build requirements
`automake-1.15`
`gcc g++`
`Make`
`Bison`
`Flex`
`GMP`
`MPC`
`MPFR`
`Texinfo`
`GRUB2`
`xorriso/libisoburn`

# Build
`./toolchain.sh` - Build the toolchain used for the rest of the build

`./build.sh` - Build all parts of the project

`./mkiso.sh` - Runs `./build.sh` then makes an iso image

`./qemu.sh` - Runs `./qemu.sh` then opens the iso in qemu -cdrom

# Cleanup
`./clean.sh` - Removes all build files
