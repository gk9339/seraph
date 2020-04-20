#ifndef _GETOPT_H
#define _GETOPT_H

#ifdef __cplusplus
extern "C" {
#endif

#define no_argument 0
#define required_argument 1
#define optinal_argument 2

struct option
{
    const char* name;
    int has_arg;
    int* flag;
    int val;
};

int getopt_long( int argc, char* const argv[], const char* optstring, const struct option* longopts, int* longindex );

#ifdef __cplusplus
}
#endif

#endif
