DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/
EXEC_PREFIX?=$(PREFIX)
BOOTDIR?=$(EXEC_PREFIX)/boot
INCLUDEDIR?=$(PREFIX)/include

CFLAGS:=$(CFLAGS) -static -ffreestanding -Wall -Wextra -pedantic -Wshadow -Wpointer-arith \
-Wcast-align -Wmissing-prototypes -Wmissing-declarations -Wredundant-decls -Wnested-externs \
-Winline -Wconversion -Wstrict-prototypes -Wno-sign-conversion -finline-functions -fno-omit-frame-pointer -O2
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
kernel/drivers/tmpfs.o \
kernel/drivers/serial.o \
kernel/drivers/ustar.o \
kernel/drivers/vga.o \
kernel/drivers/lfb.o \
kernel/drivers/pci.o \
kernel/drivers/ata.o \
kernel/drivers/mbr.o \
kernel/fs/fs.o \
kernel/fs/null.o \
kernel/fs/pipe.o \
kernel/fs/unixpipe.o \
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
kernel/proc/procfs.o \
kernel/proc/cpuinfo.o \
kernel/args.o \
kernel/bitset.o \
kernel/cbuffer.o \
kernel/kernel.o \
kernel/spinlock.o \
kernel/drivers/acpi.o \
kernel/drivers/random.o \

OBJS=\
$(ARCHDIR)/crti.o \
$(KERNEL_OBJS) \
$(ARCHDIR)/crtn.o \

.PHONY: all clean install install-headers
.SUFFIXES: .o .c .S

all: seraph.kernel

seraph.kernel: $(OBJS) $(ARCHDIR)/linker.ld
	@echo "CC kernel/version.o"
	@$(CC) -MD -c kernel/version.c -o kernel/version.o -D__TIMEZONE__=\"$(shell date +"%z")\"
	@echo "CC $(ARCHDIR)/crti.o"
	@$(CC) -T $(ARCHDIR)/linker.ld -o $@ $(CFLAGS)$(LDFLAGS) $(ARCHDIR)/crti.o $(KERNEL_OBJS) kernel/version.o $(LIBS) $(ARCHDIR)/crtn.o
	@echo "NM kernel/symbols.S"
	@$(NM) seraph.kernel -g | ./script/symbols.py > kernel/symbols.S
	@echo "CC kernel/symbols.o"
	@$(CC) -MD -c kernel/symbols.S -o kernel/symbols.o
	@echo "CC $@"
	@$(CC) -T $(ARCHDIR)/linker.ld -o $@ $(CFLAGS)$(LDFLAGS) $(ARCHDIR)/crti.o $(KERNEL_OBJS) kernel/version.o kernel/symbols.o $(LIBS) $(ARCHDIR)/crtn.o
	@echo "GRUB-FILE seraph.kernel"
	@grub-file --is-x86-multiboot seraph.kernel

.c.o:
	@echo "CC $@"
	@$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS)$(CPPFLAGS)

.S.o:
	@echo "CC $@"
	@$(CC) -MD -c $< -o $@ $(CFLAGS)$(CPPFLAGS)

clean:
	@echo "RM seraph.kernel"
	@rm -f seraph.kernel
	@echo "RM $(OBJS) kernel/symbols.o kernel/version.o"
	@rm -f $(OBJS) kernel/symbols.o kernel/version.o
	@echo "RM $(OBJS:.o=.d) kernel/symbols.d kernel/version.d"
	@rm -f $(OBJS:.o=.d) kernel/symbols.d kernel/version.d

install-headers:
	@echo "MKDIR $(DESTDIR)$(INCLUDEDIR)"
	@mkdir -p $(DESTDIR)$(INCLUDEDIR)
	@echo "CP include"
	@cp -R --preserve=timestamps include/. $(DESTDIR)$(INCLUDEDIR)/.

install: seraph.kernel
	@echo "MKDIR $(DESTDIR)$(BOOTDIR)"
	@mkdir -p $(DESTDIR)$(BOOTDIR)
	@echo "CP seraph.kernel"
	@cp seraph.kernel $(DESTDIR)$(BOOTDIR)

-include $(OBJS:.o=.d)
