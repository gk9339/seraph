#ifndef _KERNEL_ERRNO_H
#define _KERNEL_ERRNO_H

#define __sets_errno(...) int ret = __VA_ARGS__; if( ret < 0 ){ errno = -ret; ret = -1; } return ret;
extern int errno;

#endif
