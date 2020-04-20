#ifndef _ICONV_H
#define _ICONV_H

#ifdef __cplusplus
extern "C" {
#endif

typedef void* iconv_t;

iconv_t iconv_open( const char* tocode, const char* fromcode );
int iconv_close( iconv_t cd );
size_t iconv( iconv_t cd, char** inbuf, size_t* inbytesleft, char** outbuf, size_t* outbytesleft );

#ifdef __cplusplus
}
#endif

#endif
