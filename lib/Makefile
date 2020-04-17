DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/lib
EXEC_PREFIX?=$(PREFIX)
INCLUDEDIR?=$(PREFIX)/include
LIBDIR?=$(EXEC_PREFIX)/lib

CFLAGS:=$(CFLAGS) -Wall -Wextra -O3 -fPIC
DYNCFLAGS:=$(CFLAGS) -shared
LDFLAGS:=$(LDFLAGS)
LIBS:=$(LIBS)

include libm/make.config

OBJS=\
$(patsubst %, libm/%, $(LIBMOBJ)) \
libkbd/libkbd.o \
libansiterm/libansiterm.o \

LIBS=\
libm/libm.a \
libm/libm.so \
libkbd/libkbd.so \
libansiterm/libansiterm.so \

.PHONY: all clean install install-headers
.SUFFIXES: .o .c

all: $(OBJS) $(LIBS)

.c.o:
	$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS)

libm/libm.a: $(patsubst %, libm/%, $(LIBMOBJ))
	$(AR) rcs $@ $^

libm/libm.so: $(patsubst %, libm/%, $(LIBMOBJ))
	$(CC) $^ -o $@ $(DYNCFLAGS)

libkbd/libkbd.so: libkbd/libkbd.o
	$(CC) $^ -o $@ $(DYNCFLAGS)

libansiterm/libansiterm.so: libansiterm/libansiterm.o
	$(CC) $^ -o $@ $(DYNCFLAGS)

clean:
	rm -f $(LIBS)
	rm -f $(OBJS) */*.d

include:
	mkdir -p include

install-headers: include
	mkdir -p $(DESTDIR)$(INCLUDEDIR)
	cp -R --preserve=timestamps include/. $(DESTDIR)$(INCLUDEDIR)/.

install: $(LIBS)
	mkdir -p $(DESTDIR)$(LIBDIR)
	cp $(LIBS) $(DESTDIR)$(LIBDIR)

-include $(OBJS:.o=.d)
