#include <math.h>

double ldexp( double a, int exp )
{
    double out = a;

    while( exp )
    {
        out *= 2.0;
        exp--;
    }

    return out;
}
