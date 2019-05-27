DEFAULT_HOST!=../sh/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../sh/target-triplet-to-arch.sh $(HOST)

CFLAGS?=-O3 -g
CPPFLAGS?=
LDFLAGS?=
LIBS?=

DESTDIR?=
PREFIX?=/bin
EXEC_PREFIX?=$(PREFIX)
BINDIR?=$(EXEC_PREFIX)/bin
INCLUDEDIR?=$(PREFIX)/include

CFLAGS:=$(CFLAGS) -ffreestanding -Wall -Wextra
CPPFLAGS:=$(CPPFLAGS) -Iinclude
LDFLAGS:=$(LDFLAGS)
LIBS:=$(LIBS) -nostdlib -lk -lgcc


OBJS=\
init.o \

.PHONY: all clean install install-headers install-bin
.SUFFIXES: .o .c .S

all: $(OBJS)

.c.o:
	$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS) $(CPPFLAGS)

.S.o:
	$(CC) -MD -c $< -o $@ $(CFLAGS) $(CPPFLAGS)

clean:
	rm -f $(OBJS) *.o */*.o */*/*.o
	rm -f $(OBJS:.o=.d) *.d */*.d */*/*.d

install: install-headers install-bin

install-headers:
	mkdir -p $(DESTDIR)$(INCLUDEDIR)
	cp -R --preserve=timestamps include/. $(DESTDIR)$(INCLUDEDIR)/.

install-bin: $(OBJS)
	mkdir -p $(DESTDIR)$(BINDIR)
	cp $(OBJS) $(DESTDIR)$(BINDIR)

-include $(OBJS:.o=.d)
