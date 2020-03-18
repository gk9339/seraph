DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/lib
EXEC_PREFIX?=$(PREFIX)
LIBDIR?=$(EXEC_PREFIX)/lib

CFLAGS:=$(CFLAGS) -static -Wall -Werror
LDFLAGS:=$(LDFLAGS) -Wl,-static -T linker.ld

LIBS=\
ld.so \

.PHONY: all clean install install-headers install-linker
.SUFFIXES: .o .c

all: $(LIBS)

ld.so: linker.c linker.ld ../libc/libc.a
	$(CC) $(CFLAGS)$(LDFLAGS) -o $@ -Os $<

clean:
	rm -f $(LIBS)

install: install-headers install-linker

install-headers:

install-linker: $(LIBS)
	mkdir -p $(DESTDIR)$(LIBDIR)
	cp $(EXECS) $(DESTDIR)$(LIBDIR)
