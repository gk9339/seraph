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

# Project Layout
├── bin               - normal programs - dynamically linked  
│   ├── include       - normal program headers  
│   │   └── terminal  
│   ├── sh            - shell  
│   └── terminal      - VGA terminal  
├── kernel            - kernel source  
│   ├── arch          - arch specific code (asm)  
│   │   └── i386  
│   │       ├── cpu  
│   │       ├── mem  
│   │       └── proc  
│   ├── include       - kernel headers  
│   │   └── kernel  
│   ├── kernel        - main kernel source  
│   │   ├── cpu       - gdt/idt and isr/irq  
│   │   ├── dev       - cmos, fpu, and PIT  
│   │   ├── drivers   - device drivers, PS2 kbd, VGA, serial, ramdisk, ustar, ext2  
│   │   ├── fs        - fs and vfs devices  
│   │   ├── mem       - kernel memory functions / paging, shared memory  
│   │   └── proc      - process management, ELF support, syscalls, signals  
│   └── script        - scripts for kernel compilation  
├── libc              - C library  
│   ├── arch          - arch specific code (asm)  
│   │   └── i386  
│   ├── debug         - special debug functions  
│   ├── errno  
│   ├── hashtable     - hashtable ADT functions  
│   ├── include       - C library headers  
│   │   └── sys  
│   ├── ioctl  
│   ├── list          - linked list functions  
│   ├── pty           - pty system calls  
│   ├── sched  
│   ├── signal  
│   ├── stdio  
│   ├── stdlib  
│   ├── string  
│   ├── sys  
│   ├── tree          - tree ADT functions  
│   └── unistd  
├── linker            - dynamic linker  
├── sbin              - programs required for boot - statically linked  
│   ├── include  
│   └── init          - pid 0  
└── script            - other scripts for OS compilation  
└── toolchain         - seraph specific gcc/binutils  
    └── patches       - gcc/binutils patches  
