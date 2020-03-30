DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/lib
EXEC_PREFIX?=$(PREFIX)
INCLUDEDIR?=$(PREFIX)/include
LIBDIR?=$(EXEC_PREFIX)/lib

CFLAGS:=$(CFLAGS) -shared -fPIC -Wall -Wextra -O1
LDFLAGS:=$(LDFLAGS)
LIBS:=$(LIBS)

OBJS=\
libkbd/libkbd.o

LIBS=\
libkbd/libkbd.so

.PHONY: all clean install install-headers
.SUFFIXES: .o .c

all: $(OBJS) $(LIBS)

.c.o:
	$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS)

libkbd/libkbd.so: libkbd/libkbd.o
	$(CC) $^ -o $@ $(CFLAGS)

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
