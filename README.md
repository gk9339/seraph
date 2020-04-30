# seraph
![build](https://github.com/gk9339/seraph/workflows/build/badge.svg)

![Screenshot from 2020-04-19 15-27-29](https://user-images.githubusercontent.com/5243610/79682192-d9ba0f00-8252-11ea-8ba1-a384f5b35ae8.png)

### Build requirements (Archlinux)
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
`qemu-arch-extra`

### Build requirements (Ubuntu)
`automake-1.15`
`libtool`
`build-essential`
`wget`
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
`./build.sh` - Build all parts of the project (Including toolchain)

`./toolchain.sh` - Build the toolchain used for the rest of the build

`./mkiso.sh` - Runs `./build.sh` then makes an iso image

`./qemu.sh` - Runs `./mkiso.sh` then opens the iso in qemu -cdrom

### Debug
In seperate terminals:

`qemu-system-i386 -s -S -serial stdio -cdrom seraph.iso`

`gdb -ex 'tar rem :1234'` 

### Cleanup
`./clean.sh` - Removes all build files

### Project Layout
```
├── bin               - normal programs - dynamically linked  
│   ├── include       - normal program headers  
│   ├── clear         - send terminal clear command
│   ├── coreutils     - rewriting of GNU coreutils
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
