DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/bin
EXEC_PREFIX?=$(PREFIX)
BINDIR?=$(EXEC_PREFIX)/bin
INCLUDEDIR?=$(PREFIX)/include

CFLAGS:=$(CFLAGS) -Wall -Wextra -O0
LDFLAGS:=$(LDFLAGS)
LIBS:=$(LIBS)

include coreutils/make.config

OBJS=\
$(patsubst %, coreutils/%, $(COREUTILSOBJ)) \
init/init.o \
terminal/terminal.o \
sh/sh.o \
clear/clear.o \

EXECS=\
$(patsubst %, coreutils/%, $(COREUTILSEXEC)) \
init/init \
terminal/terminal \
sh/sh \
clear/clear \

.PHONY: all clean install install-headers
.SUFFIXES: .o .c

all: $(OBJS) $(EXECS)

.c.o:
	$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS)

init/init: init/init.o
	$(CC) $^ -o $@ $(CFLAGS)$(LIBS)

terminal/terminal: terminal/terminal.o
	$(CC) $^ -o $@ $(CFLAGS)$(LIBS) -lansiterm -lkbd -lm

sh/sh: sh/sh.o
	$(CC) $^ -o $@ $(CFLAGS)$(LIBS)

clear/clear: clear/clear.o
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
