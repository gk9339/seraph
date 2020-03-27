#ifndef __FILE_STRUCT_H
#define __FILE_STRUCT_H

struct _FILE
{
    int fd;

    char* read_base; //putback + get
    char* read_ptr;
    char* read_end;
    int available; //bytes in read buffer
    char* write_base; //put
    char* write_ptr;
    char* write_end;
    int bufmode; //output buffering mode
    
    char ungetc;

    int eof;

    char* _name;
};

#endif
