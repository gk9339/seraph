DEFAULT_HOST!=../script/default-host.sh
HOST?=DEFAULT_HOST
HOSTARCH!=../script/target-triplet-to-arch.sh $(HOST)

PREFIX?=/bin
EXEC_PREFIX?=$(PREFIX)
BINDIR?=$(EXEC_PREFIX)/bin
INCLUDEDIR?=$(PREFIX)/include

CFLAGS:=$(CFLAGS) -Wall -Wextra -O3
LDFLAGS:=$(LDFLAGS)
LIBS:=$(LIBS)

include coreutils/make.config
include test/make.config

OBJS=\
init/init.o \
init/reboot.o \
init/poweroff.o \
$(patsubst %,coreutils/% , $(COREUTILSOBJ))\
terminal/terminal.o \
sh/sh.o \
clear/clear.o \
$(patsubst %,test/% , $(TESTOBJ))\

EXECS=\
init/init \
init/reboot \
init/poweroff \
$(patsubst %,coreutils/% , $(COREUTILSEXEC))\
terminal/terminal \
sh/sh \
clear/clear \
$(patsubst %,test/% , $(TESTEXEC))\

.PHONY: all clean install install-headers
.SUFFIXES: .o .c .cpp

all: $(OBJS) $(EXECS)

.c.o:
	@echo "CC $@"
	@$(CC) -MD -c $< -o $@ -std=gnu11 $(CFLAGS)

.cpp.o:
	@echo "CXX $@"
	@$(CXX) -MD -c $< -o $@ $(CXXFLAGS)

init/init: init/init.o
	@echo "CC $@"
	@$(CC) $^ -o $@ $(CFLAGS)$(LIBS)

init/poweroff: init/poweroff.o
	@echo "CC $@"
	@$(CC) $^ -o $@ $(CFLAGS)$(LIBS)

init/reboot: init/reboot.o
	@echo "CC $@"
	@$(CC) $^ -o $@ $(CFLAGS)$(LIBS)

terminal/terminal: terminal/terminal.o
	@echo "CC $@"
	@$(CC) $^ -o $@ $(CFLAGS)$(LIBS) -lansiterm -lkbd -lm

sh/sh: sh/sh.o
	@echo "CC $@"
	@$(CC) $^ -o $@ $(CFLAGS)$(LIBS)

clear/clear: clear/clear.o
	@echo "CC $@"
	@$(CC) $^ -o $@ $(CFLAGS)$(LIBS)

clean:
	@echo "RM $(EXECS)"
	@rm -f $(EXECS)
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

install: $(EXECS)
	@echo "MKDIR $(DESTDIR)$(BINDIR)"
	@mkdir -p $(DESTDIR)$(BINDIR)
	@echo "CP $(EXECS)"
	@cp $(EXECS) $(DESTDIR)$(BINDIR)

-include $(OBJS:.o=.d)
