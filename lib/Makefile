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

OBJS=\
libkbd/libkbd.o \
libansiterm/libansiterm.o \

LIBS=\
libkbd/libkbd.so \
libansiterm/libansiterm.so \

.PHONY: all clean install install-headers
.SUFFIXES: .o .c

all: $(OBJS) $(LIBS)

.c.o:
	@echo "CC $@"
	@$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS)

libkbd/libkbd.so: libkbd/libkbd.o
	@echo "CC $@"
	@$(CC) $^ -o $@ $(DYNCFLAGS)

libansiterm/libansiterm.so: libansiterm/libansiterm.o
	@echo "CC $@"
	@$(CC) $^ -o $@ $(DYNCFLAGS)

clean:
	@echo "RM $(LIBS)"
	@rm -f $(LIBS)
	@echo "RM $(OBJS)"
	@rm -f $(OBJS)
	@echo "RM $(OBJS:.o=.d)"
	@rm -f $(OBJS:.o=.d)

include:
	@echo "MKDIR include"
	@mkdir -p include

install-headers: include
	@echo "MKDIR $(DESTDIR)$(INCLUDEDIR)"
	@mkdir -p $(DESTDIR)$(INCLUDEDIR)
	@echo "CP include"
	@cp -R --preserve=timestamps include/. $(DESTDIR)$(INCLUDEDIR)/.

install: $(LIBS)
	@echo "MKDIR $(DESTDIR)$(LIBDIR)"
	@mkdir -p $(DESTDIR)$(LIBDIR)
	@echo "CP $(LIBS)"
	@cp $(LIBS) $(DESTDIR)$(LIBDIR)

-include $(OBJS:.o=.d)
