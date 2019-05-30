int main( int argc, char** argv )
{
    __asm__("cli"); // General Protection Fault
    
    return 1;
}
