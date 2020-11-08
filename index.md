## About
seraph is an under development x86 operating system, created for learning purposes.

The kernel is multiboot2 compatible, supports multitasking, interprocess signalling, the ELF executable format, minimal I/O and fs drivers, VGA text and graphics mode output, and basic tty/terminal support.

The C library supports a minimal range of functions, and includes additional libraries such as hashtable, linked list and tree ADTs

The userland currently includes a VGA graphics mode terminal, a basic shell, minimal set of core utilities, and a text editor.

## Download
[Latest release ISO](https://github.com/gk9339/seraph/releases/latest/download/seraph.iso)

## Building
`./mkiso.sh` - will build the toolchain (if not already built), build the kernel, c library, and user programs, and then create an iso with the GRUB2 bootloader, which can the be burnt to a disk / USB, or run in a virtual machine. (seraph.iso)

`./qemu.sh` - will run 'seraph.iso' in qemu-system-i386 with preset hardware, and will rebuild the OS if any changes occurred beforehand

## Debugging
Create an iso file, then open it with `qemu-system-i386 -s -S -serial stdio -cdrom seraph.iso`

In another terminal open gdb and connect to the suspended qemu session with `gdb -ex 'tar rem :1234'`, select the executable to be debugged with `file <path>`, either `./kernel/seraph.kernel`, `./bin/something/something`, and add additional executable symbols with `add-symbol-file <path>` for more advanced debugging.

Resume the VM with `continue` after setting breakpoints / watchpoints

## Prerequisites
Arch (and derivatives):

`sudo pacman -S automake-1.15 gcc make curl bison flex gmp grub libisoburn python mtools qemu-arch-extra`

Ubuntu (and similar):

`sudo apt-get install automake-1.15 libtool build-essential curl bison flex libgmp3-dev libmpc-dev libmpfr-dev grub2 xorriso python3 qemu-system-x86 mtools`
