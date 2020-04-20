#ifndef _MATH_H
#define _MATH_H

#ifdef __cplusplus
extern "C" {
#endif

double floor( double x );
double pow( double x, double y );
double exp( double x );
double fmod( double x, double y );
double sqrt( double x );
float sqrtf( float x );
double fabs( double x );
float fabsf( float x );
double sin( double x );
double cos( double x );

double acos( double x );
double asin( double x );
double atan2( double y, double x );
double ceil( double x );
double cosh( double x );
double ldexp( double a, int exp );
double log( double x );
double log10( double x );
double log2( double x );
double sinh( double x );
double tan( double x );
double tanh( double x );
double atan( double x );

double frexp( double x, int* exp );
double modf( double x, double* iptr );

#ifdef __cplusplus
}
#endif

#endif
