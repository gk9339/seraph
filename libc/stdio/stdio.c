#include <stdio.h>
#include "file.h"

FILE _stdin =
{
    .fd = 0,
    .read_base = NULL,
    .read_ptr = NULL,
    .read_end = NULL,
    .available = 0,
    .write_base = NULL,
    .write_ptr = NULL,
    .write_end = NULL,
    .bufmode = _IOFBF,
    .ungetc = -1,
    .eof = 0,
    ._name = "stdin",
};

FILE _stdout =
{
    .fd = 1,
    .read_base = NULL,
    .read_ptr = NULL,
    .read_end = NULL,
    .available = -1,
    .write_base = NULL,
    .write_ptr = NULL,
    .write_end = NULL,
    .bufmode = _IOFBF,
    .ungetc = -1,
    .eof = 0,
    ._name = "stdout",
};

FILE _stderr =
{
    .fd = 2,
    .read_base = NULL,
    .read_ptr = NULL,
    .read_end = NULL,
    .available = -1,
    .write_base = NULL,
    .write_ptr = NULL,
    .write_end = NULL,
    .bufmode = _IONBF,
    .ungetc = -1,
    .eof = 0,
    ._name = "stderr",
};

FILE* stdin = &_stdin;
FILE* stdout = &_stdout;
FILE* stderr = &_stderr;
