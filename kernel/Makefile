DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/
EXEC_PREFIX?=$(PREFIX)
BOOTDIR?=$(EXEC_PREFIX)/boot
INCLUDEDIR?=$(PREFIX)/include

CFLAGS:=$(CFLAGS) -static -ffreestanding -Wall -Wextra -pedantic -Wshadow -Wpointer-arith \
-Wcast-align -Wmissing-prototypes -Wmissing-declarations -Wredundant-decls -Wnested-externs \
-Winline -Wconversion -Wstrict-prototypes -Wno-sign-conversion -O2
CPPFLAGS:=$(CPPFLAGS) -Iinclude
LDFLAGS:=$(LDFLAGS)
LIBS:=$(LIBS) -nostdlib -lk -lgcc

ARCHDIR=arch/$(HOSTARCH)

include $(ARCHDIR)/make.config

CFLAGS:=$(CFLAGS)$(KERNEL_ARCH_CFLAGS)
CPPFLAGS:=$(CPPFLAGS)$(KERNEL_ARCH_CPPFLAGS)
LDFLAGS:=$(LDFLAGS)$(KERNEL_ARCH_LDFLAGS)
LIBS:=$(LIBS)$(KERNEL_ARCH_LIBS)

KERNEL_OBJS=\
$(KERNEL_ARCH_OBJS) \
kernel/cpu/gdt.o \
kernel/cpu/idt.o \
kernel/cpu/irq.o \
kernel/cpu/isr.o \
kernel/dev/cmos.o \
kernel/dev/fpu.o \
kernel/dev/timer.o \
kernel/drivers/ext2.o \
kernel/drivers/keyboard.o \
kernel/drivers/ramdisk.o \
kernel/drivers/serial.o \
kernel/drivers/ustar.o \
kernel/drivers/vga.o \
kernel/fs/fs.o \
kernel/fs/null.o \
kernel/fs/pipe.o \
kernel/fs/zero.o \
kernel/mem/mem.o \
kernel/mem/shm.o \
kernel/proc/elf.o \
kernel/proc/process.o \
kernel/proc/pty.o \
kernel/proc/signal.o \
kernel/proc/symbols.o \
kernel/proc/syscall.o \
kernel/proc/task.o \
kernel/args.o \
kernel/bitset.o \
kernel/cbuffer.o \
kernel/kernel.o \
kernel/spinlock.o \

OBJS=\
$(ARCHDIR)/crti.o \
$(KERNEL_OBJS) \
$(ARCHDIR)/crtn.o \

LINK_LIST=\
$(LDFLAGS) \
$(ARCHDIR)/crti.o \
$(KERNEL_OBJS) \
$(LIBS) \
$(ARCHDIR)/crtn.o \

.PHONY: all clean install install-headers install-kernel
.SUFFIXES: .o .c .S

all: seraph.kernel

seraph.kernel: $(OBJS) $(ARCHDIR)/linker.ld
	$(CC) -T $(ARCHDIR)/linker.ld -o $@ $(CFLAGS) $(LINK_LIST)
	$(NM) seraph.kernel -g | ./script/symbols.py > kernel/symbols.S
	$(CC) -MD -c kernel/symbols.S -o kernel/symbols.o
	$(CC) -T $(ARCHDIR)/linker.ld -o $@ $(CFLAGS) $(LINK_LIST) kernel/symbols.o
	grub-file --is-x86-multiboot seraph.kernel

.c.o:
	$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS)$(CPPFLAGS)

.S.o:
	$(CC) -MD -c $< -o $@ $(CFLAGS)$(CPPFLAGS)

clean:
	rm -f seraph.kernel
	rm -f $(OBJS) *.o */*.o */*/*.o
	rm -f $(OBJS:.o=.d) *.d */*.d */*/*.d

install: install-headers install-kernel

install-headers:
	mkdir -p $(DESTDIR)$(INCLUDEDIR)
	cp -R --preserve=timestamps include/. $(DESTDIR)$(INCLUDEDIR)/.

install-kernel: seraph.kernel
	mkdir -p $(DESTDIR)$(BOOTDIR)
	cp seraph.kernel $(DESTDIR)$(BOOTDIR)

-include $(OBJS:.o=.d)
