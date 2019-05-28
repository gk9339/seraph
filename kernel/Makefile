DEFAULT_HOST!=../sh/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../sh/target-triplet-to-arch.sh $(HOST)

CFLAGS?=-O0 -g
CPPFLAGS?=
LDFLAGS?=
LIBS?=

DESTDIR?=
PREFIX?=/
EXEC_PREFIX?=$(PREFIX)
BOOTDIR?=$(EXEC_PREFIX)/boot
INCLUDEDIR?=$(PREFIX)/include

CFLAGS:=$(CFLAGS) -ffreestanding -Wall -Wextra
CPPFLAGS:=$(CPPFLAGS) -D__is_kernel -Iinclude
LDFLAGS:=$(LDFLAGS)
LIBS:=$(LIBS) -nostdlib -lk -lgcc

ARCHDIR=arch/$(HOSTARCH)

include $(ARCHDIR)/make.config

CFLAGS:=$(CFLAGS) $(KERNEL_ARCH_CFLAGS)
CPPFLAGS:=$(CPPFLAGS) $(KERNEL_ARCH_CPPFLAGS)
LDFLAGS:=$(LDFLAGS) $(KERNEL_ARCH_LDFLAGS)
LIBS:=$(LIBS) $(KERNEL_ARCH_LIBS)

KERNEL_OBJS=\
$(KERNEL_ARCH_OBJS) \
kernel/kernel.o \
kernel/cpu/gdt.o \
kernel/cpu/idt.o \
kernel/cpu/isr.o \
kernel/cpu/irq.o \
kernel/dev/cmos.o \
kernel/dev/timer.o \
kernel/dev/fpu.o \
kernel/io/keyboard.o \
kernel/io/tarfs.o \
kernel/io/ext2.o \
kernel/io/ramdisk.o \
kernel/mem/mem.o \
kernel/mem/shm.o \
kernel/sys/symbols.o \
kernel/sys/task.o \
kernel/sys/process.o \
kernel/sys/signal.o \
kernel/spinlock.o \
kernel/bitset.o \
kernel/args.o \
kernel/fs.o \
kernel/elf.o \

OBJS=\
$(ARCHDIR)/crti.o \
$(ARCHDIR)/crtbegin.o \
$(KERNEL_OBJS) \
$(ARCHDIR)/crtend.o \
$(ARCHDIR)/crtn.o \

LINK_LIST=\
$(LDFLAGS) \
$(ARCHDIR)/crti.o \
$(ARCHDIR)/crtbegin.o \
$(KERNEL_OBJS) \
$(LIBS) \
$(ARCHDIR)/crtend.o \
$(ARCHDIR)/crtn.o \

.PHONY: all clean install install-headers install-kernel
.SUFFIXES: .o .c .S

all: seraph.kernel

seraph.kernel: $(OBJS) $(ARCHDIR)/linker.ld
	$(CC) -T $(ARCHDIR)/linker.ld -o $@ $(CFLAGS) $(LINK_LIST)
	$(NM) seraph.kernel -g | ./symbols.py > kernel/symbols.S
	$(CC) -MD -c kernel/symbols.S -o kernel/symbols.o
	$(CC) -T $(ARCHDIR)/linker.ld -o $@ $(CFLAGS) $(LINK_LIST) kernel/symbols.o
	grub-file --is-x86-multiboot seraph.kernel

$(ARCHDIR)/crtbegin.o $(ARCHDIR)/crtend.o:
	OBJ=`$(CC) $(CFLAGS) $(LDFLAGS) -print-file-name=$(@F)` && cp "$$OBJ" $@

.c.o:
	$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS) $(CPPFLAGS)

.S.o:
	$(CC) -MD -c $< -o $@ $(CFLAGS) $(CPPFLAGS)

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
