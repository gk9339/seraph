DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/lib
EXEC_PREFIX?=$(PREFIX)
LIBDIR?=$(EXEC_PREFIX)/lib

CFLAGS:=$(CFLAGS) -static -Wall -Werror -O3
LDFLAGS:=$(LDFLAGS) -Wl,-static -T linker.ld

LIBS=\
ld.so \

.PHONY: all clean install install-headers install-linker
.SUFFIXES: .o .c

all: $(LIBS)

ld.so: linker.c linker.ld ../libc/libc.a
	@$(CC) $(CFLAGS)$(LDFLAGS) -o $@ $<
	@echo "CC $@"

clean:
	@rm -f $(LIBS)
	@echo "RM $(LIBS)"

install: $(LIBS)
	@mkdir -p $(DESTDIR)$(LIBDIR)
	@echo "MKDIR $(DESTDIR)$(LIBDIR)"
	@cp $(LIBS) $(DESTDIR)$(LIBDIR)
	@echo "CP $(LIBS)"
