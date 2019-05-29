DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

CFLAGS?=-O0 -g
LDFLAGS?=
LIBS?=

DESTDIR?=
PREFIX?=/bin
EXEC_PREFIX?=$(PREFIX)
BINDIR?=$(EXEC_PREFIX)/bin
INCLUDEDIR?=$(PREFIX)/include

CFLAGS:=$(CFLAGS) -Wall -Wextra -Iinclude
LDFLAGS:=$(LDFLAGS)
LIBS:=$(LIBS) -nostdlib -lk -lgcc

OBJS=\
init.o \

EXECS=\
init \

.PHONY: all clean install install-headers install-bin
.SUFFIXES: .o .c

all: $(OBJS) $(EXECS)

.c.o:

$(EXECS): $(OBJS)

clean:

install: install-headers install-bin

install-headers:

install-bin:

-include $(OBJS:.o=.d)
