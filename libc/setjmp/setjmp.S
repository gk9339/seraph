.global setjmp
.type setjmp, STT_FUNC
setjmp:

    pushl   %ebp
    movl    %esp,%ebp

    pushl   %edi
    movl    8(%ebp),%edi

    movl    %eax,0  (%edi)
    movl    %ebx,4  (%edi)
    movl    %ecx,8  (%edi)
    movl    %edx,12 (%edi)
    movl    %esi,16 (%edi)

    movl    -4 (%ebp),%eax
    movl    %eax,20 (%edi)

    movl    0 (%ebp),%eax
    movl    %eax,24 (%edi)

    movl    %esp,%eax
    addl    $12,%eax
    movl    %eax,28 (%edi)
    
    movl    4 (%ebp),%eax
    movl    %eax,32 (%edi)

    popl    %edi
    movl    $0,%eax
    leave
    ret

.global longjmp
.type longjmp, STT_FUNC
longjmp:
    pushl   %ebp
    movl    %esp,%ebp

    movl    8(%ebp),%edi
    movl    12(%ebp),%eax
    testl   %eax,%eax
    jne     0f
    incl    %eax
0:
    movl    %eax,0(%edi)

    movl    24(%edi),%ebp

    movl    28(%edi),%esp
    
    pushl   32(%edi)

    movl    0 (%edi),%eax
    movl    4 (%edi),%ebx
    movl    8 (%edi),%ecx
    movl    12(%edi),%edx
    movl    16(%edi),%esi
    movl    20(%edi),%edi

    ret
