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
assert/assert.o \
dirent/dirent.o \
dirent/mkdir.o \
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
unistd/access.o \
unistd/alarm.o \
unistd/unlink.o \
unistd/link.o \
unistd/chmod.o \
unistd/umask.o \
unistd/utime.o \
unistd/chown.o \
unistd/rmdir.o \
unistd/symlink.o \
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
stdio/ferror.o \
stdio/fscanf.o \
stdio/vscanf.o \
stdio/vsscanf.o \
stdio/vfscanf.o \
stdio/feof.o \
stdio/fgets.o \
stdio/freopen.o \
stdio/remove.o \
stdio/clearerr.o \
stdio/rewind.o \
stdio/getline.o \
stdlib/strtod.o \
stdlib/exit.o \
stdlib/atexit.o \
stdlib/getenv.o \
stdlib/putenv.o \
stdlib/setenv.o \
stdlib/unsetenv.o \
stdlib/qsort.o \
stdlib/abs.o \
stdlib/strtoul.o \
stdlib/atof.o \
stdlib/mbstowcs.o \
stdlib/bsearch.o \
stdlib/atol.o \
stdlib/mktemp.o \
string/strxfrm.o \
string/strcoll.o \
string/strncpy.o \
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
sys/mount.o \
libgen/basename.o \
libgen/dirname.o \
wchar/wcwidth.o \
time/gettimeofday.o \
time/strftime.o \
time/localtime.o \
time/time.o \
time/ctime.o \
pthread/pthread.o \
locale/setlocale.o \
locale/localeconv.o \
setjmp/setjmp.o \
dlfcn/dlfcn.o \
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
	@echo "AR $@"
	@$(AR) rcs $@ $^

libc.so: $(OBJS) libm.a
	@echo "MKDIR $(DESTDIR)$(LIBDIR)"
	@mkdir -p $(DESTDIR)$(LIBDIR)
	@echo "AS $(ARCHDIR)/crt0.o"
	@$(AS) $(ARCHDIR)/crt0.S -o $(ARCHDIR)/crt0.o
	@echo "AS $(ARCHDIR)/crti.o"
	@$(AS) $(ARCHDIR)/crti.S -o $(ARCHDIR)/crti.o
	@echo "AS $(ARCHDIR)/crtn.o"
	@$(AS) $(ARCHDIR)/crtn.S -o $(ARCHDIR)/crtn.o
	@echo "CP $(ARCHDIR)/crt0.o"
	@cp $(ARCHDIR)/crt0.o $(DESTDIR)$(LIBDIR)
	@echo "CP $(ARCHDIR)/crti.o"
	@cp $(ARCHDIR)/crti.o $(DESTDIR)$(LIBDIR)
	@echo "CP $(ARCHDIR)/crtn.o"
	@cp $(ARCHDIR)/crtn.o $(DESTDIR)$(LIBDIR)
	@echo "CC crtbegin.o"
	@echo "CP crtbegin.o"
	@OBJ=`$(CC) $(CFLAGS) $(LDFLAGS) -print-file-name=crtbegin.o` && cp "$$OBJ" $(DESTDIR)$(LIBDIR)
	@echo "CC crtend.o"
	@echo "CP crtend.o"
	@OBJ=`$(CC) $(CFLAGS) $(LDFLAGS) -print-file-name=crtend.o` && cp "$$OBJ" $(DESTDIR)$(LIBDIR)
	@echo "CC $@"
	@$(CC) -nodefaultlibs -o $@ $(CFLAGS) -shared $^ -lgcc

libk.a: $(LIBK_OBJS)
	@echo "AR $@"
	@$(AR) rcs $@ $^

libm.a: $(patsubst %, libm/%, $(LIBMOBJ))
	@echo "AR $@"
	@$(AR) rcs $@ $^

libm.so: $(patsubst %, libm/%, $(LIBMOBJ)) | libc.so
	@echo "CC $@"
	@$(CC) -nodefaultlibs -o $@ $(CFLAGS) -shared $^

libssp.a: libssp/ssp.o
	@echo "AR $@"
	@$(AR) rcs $@ $^

libssp.so: libssp/ssp.o | libc.so
	@echo "CC $@"
	@$(CC) -nodefaultlibs -o $@ $(CFLAGS) -shared $^

libssp_nonshared.a: libssp/ssp_local.o libssp/ssp.o
	@echo "AR $@"
	@$(AR) rcs $@ $^

.c.o:
	@echo "CC $@"
	@$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS)$(CPPFLAGS)

.S.o:
	@echo "CC $@"
	@$(CC) -MD -c $< -o $@ $(CFLAGS)$(CPPFLAGS)

.c.libk.o:
	@echo "CC $@"
	@$(CC) -MD -c $< -o $@ -std=gnu11 $(LIBK_CFLAGS)$(LIBK_CPPFLAGS)

.S.libk.o:
	@echo "CC $@"
	@$(CC) -MD -c $< -o $@ $(LIBK_CFLAGS)$(LIBK_CPPFLAGS)

clean:
	@echo "RM $(BINARIES)"
	@rm -f $(BINARIES)
	@echo "RM $(CRTS)"
	@rm -f $(CRTS)
	@echo "RM $(OBJS:.o=.o) $(LIBK_OBJS:.o=.o) $(patsubst %, libm/%, $(LIBMOBJ:.o=.o))"
	@rm -f $(OBJS:.o=.o) $(LIBK_OBJS:.o=.o) $(patsubst %, libm/%, $(LIBMOBJ:.o=.o))
	@echo "RM $(OBJS:.o=.d) $(LIBK_OBJS:.o=.d) $(patsubst %, libm/%, $(LIBMOBJ:.o=.d))"
	@rm -f $(OBJS:.o=.d) $(LIBK_OBJS:.o=.d) $(patsubst %, libm/%, $(LIBMOBJ:.o=.d))

install-headers:
	@echo "MKDIR $(DESTDIR)$(INCLUDEDIR)"
	@mkdir -p $(DESTDIR)$(INCLUDEDIR)
	@echo "CP include"
	@cp -R --preserve=timestamps include/. $(DESTDIR)$(INCLUDEDIR)/.

install: $(BINARIES)
	@echo "MKDIR $(DESTDIR)$(LIBDIR)"
	@mkdir -p $(DESTDIR)$(LIBDIR)
	@echo "CP $(BINARIES)"
	@cp $(BINARIES) $(DESTDIR)$(LIBDIR)

-include $(OBJS:.o=.d)
-include $(LIBK_OBJS:.o=.d)
