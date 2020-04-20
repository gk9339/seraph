#include <locale.h>

char* setlocale( int category __attribute__((unused)), const char* locale __attribute__((unused)) )
{
    return "en_US";
}
