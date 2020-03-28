DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/bin
EXEC_PREFIX?=$(PREFIX)
BINDIR?=$(EXEC_PREFIX)/bin
INCLUDEDIR?=$(PREFIX)/include

CFLAGS:=$(CFLAGS) -static -Wall -Wextra -O3
LDFLAGS:=$(LDFLAGS)
LIBS:=$(LIBS) -lc

OBJS=\
terminal/terminal.o \
terminal/keyboard.o \
sh/sh.o \
sh/ls.o \

EXECS=\
terminal/terminal \
sh/sh \

.PHONY: all clean install install-headers install-bin
.SUFFIXES: .o .c

all: $(OBJS) $(EXECS)

.c.o:
	$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS)

terminal/terminal: terminal/terminal.o terminal/keyboard.o
	$(CC) $^ -o $@ $(CFLAGS)$(LIBS)

sh/sh: sh/sh.o sh/ls.o
	$(CC) $^ -o $@ $(CFLAGS)$(LIBS)

clean:
	rm -f $(EXECS)
	rm -f $(OBJS) */*.d

include:
	mkdir -p include

install-headers: include
	mkdir -p $(DESTDIR)$(INCLUDEDIR)
	cp -R --preserve=timestamps include/. $(DESTDIR)$(INCLUDEDIR)/.

install: $(EXECS)
	mkdir -p $(DESTDIR)$(BINDIR)
	cp $(EXECS) $(DESTDIR)$(BINDIR)

-include $(OBJS:.o=.d)
