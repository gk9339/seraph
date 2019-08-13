DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/sbin
EXEC_PREFIX?=$(PREFIX)
SBINDIR?=$(EXEC_PREFIX)/sbin
INCLUDEDIR?=$(PREFIX)/include

CFLAGS:=$(CFLAGS) -Wall -Wextra
LDFLAGS:=$(LDFLAGS)
LIBS:=$(LIBS) -lc

OBJS=\
init.o \

EXECS=\
init \

.PHONY: all clean install install-headers install-sbin
.SUFFIXES: .o .c

all: $(OBJS) $(EXECS)

.c.o:
	$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS)

init: init.o
	$(CC) $< -o $@ $(CFLAGS)$(LIBS)

clean:
	rm -f $(EXECS)
	rm -f $(OBJS) *.d

install: install-headers install-sbin

install-headers:

install-sbin: $(EXECS)
	mkdir -p $(DESTDIR)$(SBINDIR)
	cp $(EXECS) $(DESTDIR)$(SBINDIR)

-include $(OBJS:.o=.d)
