DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/libc
EXEC_PREFIX?=$(PREFIX)
INCLUDEDIR?=$(PREFIX)/include
LIBDIR?=$(EXEC_PREFIX)/lib

CFLAGS:=$(CFLAGS) -ffreestanding -Wall -Wextra -pedantic -Wshadow -Wpointer-arith \
-Wcast-align -Wmissing-prototypes -Wmissing-declarations -Wredundant-decls -Wnested-externs \
-Winline -Wconversion -Wstrict-prototypes -Wno-sign-conversion -O2
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
string/strncmp.o \
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
unistd/close.o \
unistd/lseek.o \
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
stdlib/getenv.o \
stdlib/putenv.o \
stdlib/setenv.o \
stdlib/unsetenv.o \
stdlib/stub.o \
errno/errno.o \
stdio/stdio.o \
sched/sched_yield.o \
pty/pty.o \
signal/signal.o \
sys/wait.o \
sys/fswait.o \
sys/setheap.o \
sys/stat.o \
sys/mmap.o \
debug/debugvfstree.o \
debug/debugproctree.o \
debug/debugprint.o \

OBJS=\
$(FREEOBJS) \
$(HOSTEDOBJS) \

LIBK_OBJS=$(FREEOBJS:.o=.libk.o)

BINARIES=libc.a libk.a libc.so

CRTS=\
$(ARCH_CRTS) \

.PHONY: all clean install install-headers
.SUFFIXES: .o .libk.o .c .S

all: $(BINARIES) $(CRTS)

libc.a: $(OBJS)
	$(AR) rcs $@ $(OBJS)

libc.so: $(OBJS)
	mkdir -p $(DESTDIR)$(LIBDIR)
	$(AS) $(ARCHDIR)/crt0.S -o crt0.o
	$(AS) $(ARCHDIR)/crti.S -o crti.o
	$(AS) $(ARCHDIR)/crtn.S -o crtn.o
	cp crt0.o $(DESTDIR)$(LIBDIR)
	cp crti.o $(DESTDIR)$(LIBDIR)
	cp crtn.o $(DESTDIR)$(LIBDIR)
	OBJ=`$(CC) $(CFLAGS) $(LDFLAGS) -print-file-name=crtbegin.o` && cp "$$OBJ" $(DESTDIR)$(LIBDIR)
	OBJ=`$(CC) $(CFLAGS) $(LDFLAGS) -print-file-name=crtend.o` && cp "$$OBJ" $(DESTDIR)$(LIBDIR)
	$(CC) -nodefaultlibs -o $@ $(CFLAGS) -shared -fPIC $^ -lgcc

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

clean:
	rm -f $(BINARIES) *.a
	rm -f $(CRTS) *.o
	rm -f $(OBJS) $(LIBK_OBJS) *.o */*.o */*/*.o
	rm -f $(OBJS:.o=.d) $(LIBK_OBJS:.o=.d) *.d */*.d */*/*.d

install: $(BINARIES)
	mkdir -p $(DESTDIR)$(LIBDIR)
	cp $(BINARIES) $(DESTDIR)$(LIBDIR)

install-headers:
	mkdir -p $(DESTDIR)$(INCLUDEDIR)
	cp -R --preserve=timestamps include/. $(DESTDIR)$(INCLUDEDIR)/.

-include $(OBJS:.o=.d)
-include $(LIBK_OBJS:.o=.d)
