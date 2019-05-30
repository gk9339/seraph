DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/bin
EXEC_PREFIX?=$(PREFIX)
BINDIR?=$(EXEC_PREFIX)/bin
INCLUDEDIR?=$(PREFIX)/include

CFLAGS:=$(CFLAGS) -Wall -Wextra
LDFLAGS:=$(LDFLAGS)
LIBS:=$(LIBS) -lc -lgcc

OBJS=\
init.o \

EXECS=\
init \

.PHONY: all clean install install-headers install-bin
.SUFFIXES: .o .c

all: $(OBJS) $(EXECS)

.c.o:
	$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS)

$(EXECS): $(OBJS)
	$(CC) -static $< -o $@ $(CFLAGS) $(LIBS)

clean:
	rm -f $(EXECS)
	rm -f $(OBJS) *.d

install: install-headers install-bin

install-headers:

install-bin: $(EXECS)
	mkdir -p $(DESTDIR)$(BINDIR)
	cp $(EXECS) $(DESTDIR)$(BINDIR)

-include $(OBJS:.o=.d)
