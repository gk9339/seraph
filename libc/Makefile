DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/libc
EXEC_PREFIX?=$(PREFIX)
INCLUDEDIR?=$(PREFIX)/include
LIBDIR?=$(EXEC_PREFIX)/lib

CFLAGS:=$(CFLAGS) -ffreestanding -fPIC -Wall -Wextra -pedantic -Wshadow -Wpointer-arith \
-Wcast-align -Wmissing-prototypes -Wmissing-declarations -Wredundant-decls -Wnested-externs \
-Winline -Wconversion -Wstrict-prototypes -Wno-sign-conversion
CPPFLAGS:=$(CPPFLAGS) -Iinclude
LIBK_CFLAGS:=$(CFLAGS) -fno-omit-frame-pointer -O2
LIBK_CPPFLAGS:=$(CPPFLAGS) -D__is_libk
CFLAGS:=$(CFLAGS) -O3

ARCHDIR=arch/$(HOSTARCH)

include $(ARCHDIR)/make.config
include libm/make.config

CFLAGS:=$(CFLAGS)$(ARCH_CFLAGS)
CPPFLAGS:=$(CPPFLAGS)$(ARCH_CPPFLAGS)
LIBK_CFLAGS:=$(LIBK_CFLAGS)$(KERNEL_ARCH_CFLAGS)
LIBK_CPPFLAGS:=$(LIBK_CPPFLAGS)$(KERNEL_ARCH_CPPFLAGS)

FREEOBJS=\
$(ARCH_FREEOBJS) \
stdio/printf.o \
stdio/putc.o \
stdio/fwrite.o \
stdio/sprintf.o \
stdio/vfprintf.o \
stdio/vprintf.o \
stdio/vsnprintf.o \
stdio/vsprintf.o \
stdio/fmemopen.o \
stdlib/abort.o \
stdlib/malloc.o \
stdlib/atoi.o \
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
string/strerror.o \
string/strsignal.o \
list/list.o \
tree/tree.o \
hashtable/hashtable.o \
errno/errno.o \
ctype/ctype.o \
libssp/ssp_local.o \
libssp/ssp.o \

HOSTEDOBJS=\
$(ARCH_HOSTEDOBJS) \
entry.o \
exit.o \
dirent/dirent.o \
unistd/close.o \
unistd/lseek.o \
unistd/open.o \
unistd/usleep.o \
unistd/sleep.o \
unistd/sbrk.o \
unistd/read.o \
unistd/execv.o \
unistd/fork.o \
unistd/fcntl.o \
unistd/getpid.o \
unistd/getppid.o \
unistd/getuid.o \
unistd/setuid.o \
unistd/getgid.o \
unistd/setpgid.o \
unistd/setsid.o \
unistd/write.o \
unistd/dup.o \
unistd/dup2.o \
unistd/pipe.o \
unistd/chdir.o \
unistd/gethostname.o \
unistd/sethostname.o \
unistd/getopt.o \
unistd/getopt_long.o \
unistd/isatty.o \
unistd/getwd.o \
unistd/getcwd.o \
unistd/readlink.o \
ioctl/ioctl.o \
stdio/fread.o \
stdio/fseek.o \
stdio/fflush.o \
stdio/fclose.o \
stdio/fopen.o \
stdio/fdopen.o \
stdio/fprintf.o \
stdio/snprintf.o \
stdio/getchar.o \
stdio/fgetc.o \
stdio/getc.o \
stdio/stdio.o \
stdio/stub.o \
stdio/ungetc.o \
stdio/perror.o \
stdio/putchar.o \
stdio/puts.o \
stdio/fputs.o \
stdio/fputc.o \
stdio/setvbuf.o \
stdio/scanf.o \
stdio/sscanf.o \
stdio/fscanf.o \
stdio/vscanf.o \
stdio/vsscanf.o \
stdio/vfscanf.o \
stdlib/strtod.o \
stdlib/exit.o \
stdlib/atexit.o \
stdlib/getenv.o \
stdlib/putenv.o \
stdlib/setenv.o \
stdlib/unsetenv.o \
stdlib/qsort.o \
stdlib/abs.o \
string/strxfrm.o \
string/strcoll.o \
sched/sched_yield.o \
pty/pty.o \
signal/signal.o \
signal/kill.o \
signal/raise.o \
sys/wait.o \
sys/fswait.o \
sys/setheap.o \
sys/stat.o \
sys/fstat.o \
sys/mmap.o \
sys/uname.o \
sys/reboot.o \
libgen/basename.o \
libgen/dirname.o \
wchar/wcwidth.o \
time/gettimeofday.o \
time/strftime.o \
pthread/pthread.o \
locale/setlocale.o \
setjmp/setjmp.o \
debug/debugvfstree.o \
debug/debugproctree.o \
debug/debugprint.o \

OBJS=\
$(FREEOBJS) \
$(HOSTEDOBJS) \

LIBK_OBJS=$(FREEOBJS:.o=.libk.o)

BINARIES=libc.a libk.a libc.so libm.a libm.so libssp.a libssp.so libssp_nonshared.a

CRTS=\
$(ARCH_CRTS) \

.PHONY: all clean install install-headers
.SUFFIXES: .o .libk.o .c .S

all: $(BINARIES) $(CRTS)

libc.a: $(OBJS)
	$(AR) rcs $@ $^

libc.so: $(OBJS) libm.a
	mkdir -p $(DESTDIR)$(LIBDIR)
	$(AS) $(ARCHDIR)/crt0.S -o crt0.o
	$(AS) $(ARCHDIR)/crti.S -o crti.o
	$(AS) $(ARCHDIR)/crtn.S -o crtn.o
	cp crt0.o $(DESTDIR)$(LIBDIR)
	cp crti.o $(DESTDIR)$(LIBDIR)
	cp crtn.o $(DESTDIR)$(LIBDIR)
	OBJ=`$(CC) $(CFLAGS) $(LDFLAGS) -print-file-name=crtbegin.o` && cp "$$OBJ" $(DESTDIR)$(LIBDIR)
	OBJ=`$(CC) $(CFLAGS) $(LDFLAGS) -print-file-name=crtend.o` && cp "$$OBJ" $(DESTDIR)$(LIBDIR)
	$(CC) -nodefaultlibs -o $@ $(CFLAGS) -shared $^ -lgcc
	cd ..; ./libstdcxx.sh

libk.a: $(LIBK_OBJS)
	$(AR) rcs $@ $^

libm.a: $(patsubst %, libm/%, $(LIBMOBJ))
	$(AR) rcs $@ $^

libm.so: $(patsubst %, libm/%, $(LIBMOBJ))
	$(CC) -nodefaultlibs -o $@ $(CFLAGS) -shared $^

libssp.a: libssp/ssp.o
	$(AR) rcs $@ $^

libssp.so: libssp/ssp.o
	$(CC) -nodefaultlibs -o $@ $(CFLAGS) -shared $^

libssp_nonshared.a: libssp/ssp_local.o libssp/ssp.o
	$(AR) rcs $@ $^

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

install-headers:
	mkdir -p $(DESTDIR)$(INCLUDEDIR)
	cp -R --preserve=timestamps include/. $(DESTDIR)$(INCLUDEDIR)/.

install: $(BINARIES)
	mkdir -p $(DESTDIR)$(LIBDIR)
	cp $(BINARIES) $(DESTDIR)$(LIBDIR)

-include $(OBJS:.o=.d)
-include $(LIBK_OBJS:.o=.d)
