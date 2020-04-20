#ifndef _DEBUG_H
#define _DEBUG_H

#ifdef __cplusplus
extern "C" {
#endif

int debugvfstree( char** );
int debugproctree( char** );
int debugprintf( char*, const char* format, ... );
int debugprint( char* );

#ifdef __cplusplus
}
#endif

#endif
