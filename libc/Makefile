DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/libc
EXEC_PREFIX?=$(PREFIX)
INCLUDEDIR?=$(PREFIX)/include
LIBDIR?=$(EXEC_PREFIX)/lib

CFLAGS:=$(CFLAGS) -ffreestanding -Wall -Wextra -pedantic -Wshadow -Wpointer-arith \
-Wcast-align -Wmissing-prototypes -Wmissing-declarations -Wredundant-decls -Wnested-externs \
-Winline -Wconversion -Wstrict-prototypes -Wno-sign-conversion -O3
CPPFLAGS:=$(CPPFLAGS) -Iinclude
LIBK_CFLAGS:=$(CFLAGS)
LIBK_CPPFLAGS:=$(CPPFLAGS) -D__is_libk

ARCHDIR=arch/$(HOSTARCH)

include $(ARCHDIR)/make.config

CFLAGS:=$(CFLAGS)$(ARCH_CFLAGS)
CPPFLAGS:=$(CPPFLAGS)$(ARCH_CPPFLAGS)
LIBK_CFLAGS:=$(LIBK_CFLAGS)$(KERNEL_ARCH_CFLAGS)
LIBK_CPPFLAGS:=$(LIBK_CPPFLAGS)$(KERNEL_ARCH_CPPFLAGS)

FREEOBJS=\
$(ARCH_FREEOBJS) \
stdio/printf.o \
stdio/putchar.o \
stdio/puts.o \
stdlib/abort.o \
stdlib/malloc.o \
string/memcmp.o \
string/memcpy.o \
string/memmove.o \
string/memset.o \
string/strlen.o \
string/strcmp.o \
string/strcpy.o \
string/strdup.o \
string/strtok.o \
string/strchr.o \
string/strstr.o \
string/memchr.o \
string/strcat.o \
list/list.o \
tree/tree.o \
hashtable/hashtable.o \

HOSTEDOBJS=\
$(ARCH_HOSTEDOBJS) \
entry.o \
exit.o \
unistd/stub.o \
unistd/open.o \
unistd/usleep.o \
unistd/sleep.o \
unistd/sbrk.o \
unistd/read.o \
unistd/execv.o \
unistd/fork.o \
unistd/getpid.o \
unistd/getuid.o \
unistd/setuid.o \
unistd/getgid.o \
unistd/setpgid.o \
unistd/setsid.o \
unistd/write.o \
unistd/dup.o \
unistd/dup2.o \
ioctl/ioctl.o \
stdio/stdio.o \
stdio/stub.o \
stdlib/stub.o \
errno/errno.o \
stdio/stdio.o \
sched/sched_yield.o \
pty/pty.o \
signal/signal.o \
sys/wait.o \
sys/fswait.o \
debug/debugvfstree.o \
debug/debugproctree.o \

OBJS=\
$(FREEOBJS) \
$(HOSTEDOBJS) \

LIBK_OBJS=$(FREEOBJS:.o=.libk.o)

BINARIES=libc.a libk.a

CRTS=\
crt0.o \
crti.o \
crtn.o \

.PHONY: all clean install install-headers install-libs
.SUFFIXES: .o .libk.o .c .S

all: $(BINARIES) $(CRTS)

libc.a: $(OBJS)
	$(AR) rcs $@ $(OBJS)

libk.a: $(LIBK_OBJS)
	$(AR) rcs $@ $(LIBK_OBJS)

.c.o:
	$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS)$(CPPFLAGS)

.c.S:
	$(CC) -MD -c $< -o $@ $(CFLAGS)$(CPPFLAGS)

.c.libk.o:
	$(CC) -MD -c $< -o $@ -std=gnu11 $(LIBK_CFLAGS)$(LIBK_CPPFLAGS)

.S.libk.o:
	$(CC) -MD -c $< -o $@ $(LIBK_CFLAGS)$(LIBK_CPPFLAGS)

crt%.o: crt%.S
	$(AS) $< -o $@

clean:
	rm -f $(BINARIES) *.a
	rm -f $(CRTS) *.o
	rm -f $(OBJS) $(LIBK_OBJS) *.o */*.o */*/*.o
	rm -f $(OBJS:.o=.d) $(LIBK_OBJS:.o=.d) *.d */*.d */*/*.d

install: install-headers install-libs

install-headers:
	mkdir -p $(DESTDIR)$(INCLUDEDIR)
	cp -R --preserve=timestamps include/. $(DESTDIR)$(INCLUDEDIR)/.

install-libs: $(BINARIES) $(CRTS)
	mkdir -p $(DESTDIR)$(LIBDIR)
	cp $(BINARIES) $(DESTDIR)$(LIBDIR)
	cp $(CRTS) $(DESTDIR)$(LIBDIR)

-include $(OBJS:.o=.d)
-include $(LIBK_OBJS:.o=.d)
