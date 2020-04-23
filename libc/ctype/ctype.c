#include <ctype.h>

char _ctype_[256] =
{
    _C, _C, _C, _C, _C, _C, _C, _C,
    _C, _C|_S, _C|_S, _C|_S, _C|_S, _C|_S, _C, _C,
    _C, _C, _C, _C, _C, _C, _C, _C,
    _C, _C, _C, _C, _C, _C, _C, _C,
    _S|_B, _P, _P, _P, _P, _P, _P, _P,
    _P, _P, _P, _P, _P, _P, _P, _P,
    _N, _N, _N, _N, _N, _N, _N, _N,
    _N, _N, _P, _P, _P, _P, _P, _P,
    _P, _U|_X, _U|_X, _U|_X, _U|_X, _U|_X, _U|_X, _U,
    _U, _U, _U, _U, _U, _U, _U, _U,
    _U, _U, _U, _U, _U, _U, _U, _U,
    _U, _U, _U, _P, _P, _P, _P, _P,
    _P, _L|_X, _L|_X, _L|_X, _L|_X, _L|_X, _L|_X, _L,
    _L, _L, _L, _L, _L, _L, _L, _L,
    _L, _L, _L, _L, _L, _L, _L, _L,
    _L, _L, _L, _P, _P, _P, _P, _C
};

int isalnum( int c )
{
    return isalpha(c) || isdigit(c);
}

int isalpha( int c )
{
    return ((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z'));
}

int isascii( int c )
{
    return (c <= 0x7f);
}

int iscntrl( int c )
{
    return ((c >= 0 && c <= 0x1f) || (c == 0x7f));
}

int isdigit( int c )
{
    return (c >= '0' && c <= '9');
}

int isgraph( int c )
{
    return (c >= '!' && c <= '~');
}

int islower( int c )
{
    return (c >= 'a' && c <= 'z');
}

int isprint( int c )
{
    return isgraph(c) || c == ' ';
}

int ispunct( int c )
{
    return isgraph(c) && !isalnum(c);
}

int isspace( int c )
{
    return (c == '\f' || c == '\n' || c == '\r' || c == '\t' || c == '\v' || c == ' ');
}

int isupper( int c )
{
    return (c >= 'A' && c <= 'Z');
}

int isxdigit( int c )
{
    return ((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F'));
}

int tolower( int c )
{
    if( c >= 'A' && c <= 'Z' )
    {
        return c - 'A' + 'a';
    }

    return c;
}

int toupper( int c )
{
    if( c >= 'a' && c <= 'z' )
    {
        return c - 'a' + 'A';
    }

    return c;
}
