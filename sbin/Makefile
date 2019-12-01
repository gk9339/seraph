DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/sbin
EXEC_PREFIX?=$(PREFIX)
SBINDIR?=$(EXEC_PREFIX)/sbin
INCLUDEDIR?=$(PREFIX)/include

CFLAGS:=$(CFLAGS) -static -Wall -Wextra -O3
LDFLAGS:=$(LDFLAGS)
LIBS:=$(LIBS) -lc

OBJS=\
init/init.o \

EXECS=\
init/init \

.PHONY: all clean install install-headers install-sbin
.SUFFIXES: .o .c

all: $(OBJS) $(EXECS)

.c.o:
	$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS)

init/init: init/init.o
	$(CC) $^ -o $@ $(CFLAGS)$(LIBS)

clean:
	rm -f $(EXECS)
	rm -f $(OBJS) *.d

install: install-headers install-sbin

install-headers:
	mkdir -p $(DESTDIR)$(INCLUDEDIR)
	cp -R --preserve=timestamps include/. $(DESTDIR)$(INCLUDEDIR)/.

install-sbin: $(EXECS)
	mkdir -p $(DESTDIR)$(SBINDIR)
	cp $(EXECS) $(DESTDIR)$(SBINDIR)

-include $(OBJS:.o=.d)
