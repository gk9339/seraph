#include <kernel/version.h>

#ifndef __TIMEZONE__
#define __TIMEZONE__ "+????"
#endif

char* _kernel_name = "seraph";

int _kernel_version_major = 0;
int _kernel_version_minor = 0;
int _kernel_version_lower = 5;

char* _kernel_arch = "i686";

char* _kernel_build_date = __DATE__;
char* _kernel_build_time = __TIME__;
char* _kernel_build_timezone = __TIMEZONE__;
