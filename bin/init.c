int main( int argc, char** argv )
{
    asm volatile("cli");

    while(1){}

    return 1;
}
