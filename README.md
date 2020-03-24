# seraph

# Build requirements (Archlinux)
`automake-1.15`
`gcc`
`make`
`wget`
`bison`
`flex`
`gmp`
`grub`
`libisoburn`
`python`
`mtools`

# Build
`./build.sh` - Build all parts of the project (Including toolchain)

`./toolchain.sh` - Build the toolchain used for the rest of the build

`./mkiso.sh` - Runs `./build.sh` then makes an iso image

`./qemu.sh` - Runs `./mkiso.sh` then opens the iso in qemu -cdrom

# Debug
In seperate terminals:
`qemu-system-i386 -s -S -serial stdio -cdrom seraph.iso`
`gdb -ex 'tar rem :1234'` 

# Cleanup
`./clean.sh` - Removes all build files
