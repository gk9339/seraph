# seraph
![build](https://github.com/gk9339/seraph/workflows/build/badge.svg)

![Screenshot from 2020-05-04 14-45-39](https://user-images.githubusercontent.com/5243610/80942024-5e418b80-8e16-11ea-9da7-94a18f2c6e22.png)

### Build requirements (Arch)
`gcc`
`make`
`curl`
`bison`
`flex`
`gmp`
`grub`
`libisoburn`
`python`
`mtools`
`qemu-arch-extra`

### Build requirements (Ubuntu)
`libtool`
`build-essential`
`curl`
`bison`
`flex`
`libgmp3-dev`
`libmpc-dev`
`libmpfr-dev`
`grub2`
`xorriso`
`python3`
`qemu-system-x86`
`mtools`

### Build
`./build.sh` - Build all parts of the project (Including toolchain if not built)

`./toolchain.sh` - Build the toolchain used for the rest of the build

`./mkiso.sh` - Runs `./build.sh` then makes an iso image

`./qemu.sh` - Runs `./mkiso.sh` then opens the iso in qemu -cdrom

### Debug
In seperate terminals:

1:

`qemu-system-i386 -s -S -serial stdio -cdrom seraph.iso`

2:

`gdb -ex 'tar rem :1234'`

`file <path>` and / or `add-symbol-file <path>`

### Cleanup
`./clean.sh` - Removes all build files

### Project Layout
```
├── bin               - normal programs - dynamically linked  
│   ├── include       - normal program headers  
│   ├── clear         - send terminal clear command
│   ├── coreutils     - rewriting of GNU coreutils
│   ├── edit          - text editor
│   ├── init          - pid 1
│   ├── sh            - shell  
│   ├── terminal      - framebuffer terminal
│   └── test          - testing binaries  
├── kernel            - kernel source  
│   ├── include       
│   │   └── kernel    - kernel headers
│   ├── arch          - arch specific code (asm)  
│   │   └── i386  
│   │       ├── cpu  
│   │       ├── mem  
│   │       └── proc  
│   ├── kernel        - main kernel source  
│   │   ├── cpu       - gdt/idt and isr/irq  
│   │   ├── dev       - cmos, fpu, and PIT  
│   │   ├── drivers   - device drivers, PS2 kbd, VGA, serial, ramdisk, ustar, ext2  
│   │   ├── fs        - fs and vfs devices  
│   │   ├── mem       - kernel memory functions / paging, shared memory  
│   │   └── proc      - process management, ELF support, syscalls, signals  
│   └── script        - scripts for kernel compilation  
├── lib               - other shared libraries 
│   ├── include       - other library headers
│   ├── libansiterm   - processing ANSI escape characters etc
│   ├── libkbd        - conversion of keyboard scancodes
├── libc              - C library  
│   ├── include       - C library headers  
│   │   └── sys  
│   ├── arch          - arch specific code (asm)  
│   │   └── i386  
│   ├── assert
│   ├── ctype
│   ├── debug         - special debug functions  
│   ├── dirent
│   ├── dlfcn
│   ├── errno  
│   ├── hashtable     - hashtable ADT functions  
│   ├── ioctl  
│   ├── libgen   
│   ├── libm          - C math library
│   ├── libssp
│   ├── list          - linked list functions  
│   ├── locale
│   ├── pthread
│   ├── pty           - pty system calls  
│   ├── sched  
│   ├── setjmp
│   ├── signal  
│   ├── stdio  
│   ├── stdlib  
│   ├── string  
│   ├── sys  
│   ├── time
│   ├── tree          - tree ADT functions  
│   ├── unistd
│   └── wchar 
├── linker            - dynamic linker  
├── ports             - porting software, gmp, mpfr, mpc, binutils, gcc (early testing)
├── script            - other scripts for OS compilation  
└── toolchain         - seraph specific gcc/binutils  
    └── patches       - gcc/binutils patches  
```
