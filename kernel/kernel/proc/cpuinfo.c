#include <stdint.h>
#include <kernel/fs.h>
#include <stdlib.h>
#include <string.h>
#include <kernel/process.h>

#define ECX_SSE3           (1 << 0)  // Streaming SIMD Extensions 3
#define ECX_PCLMULQDQ      (1 << 1)  // PCLMULQDQ Instruction
#define ECX_DTES64         (1 << 2)  // 64-Bit Debug Store Area
#define ECX_MONITOR        (1 << 3)  // MONITOR/MWAIT
#define ECX_DS_CPL         (1 << 4)  // CPL Qualified Debug Store
#define ECX_VMX            (1 << 5)  // Virtual Machine Extensions
#define ECX_SMX            (1 << 6)  // Safer Mode Extensions
#define ECX_EST            (1 << 7)  // Enhanced SpeedStep Technology
#define ECX_TM2            (1 << 8)  // Thermal Monitor 2
#define ECX_SSSE3          (1 << 9)  // Supplemental Streaming SIMD Extensions 3
#define ECX_CNXT_ID        (1 << 10) // L1 Context ID
#define ECX_SDBG           (1 << 11) // Silicon Debug Interface
#define ECX_FMA            (1 << 12) // Fused Multiply Add
#define ECX_CX16           (1 << 13) // CMPXCHG16B Instruction
#define ECX_XTPR           (1 << 14) // xTPR Update Control
#define ECX_PDCM           (1 << 15) // Perf/Debug Capability MSR
#define ECX_PCID           (1 << 17) // Process-context Identifiers
#define ECX_DCA            (1 << 18) // Direct Cache Access
#define ECX_SSE41          (1 << 19) // Streaming SIMD Extensions 4.1
#define ECX_SSE42          (1 << 20) // Streaming SIMD Extensions 4.2
#define ECX_X2APIC         (1 << 21) // Extended xAPIC Support
#define ECX_MOVBE          (1 << 22) // MOVBE Instruction
#define ECX_POPCNT         (1 << 23) // POPCNT Instruction
#define ECX_TSC            (1 << 24) // Local APIC supports TSC Deadline
#define ECX_AES            (1 << 25) // AESNI Instruction
#define ECX_XSAVE          (1 << 26) // XSAVE/XSTOR States
#define ECX_OSXSAVE        (1 << 27) // OS Enabled Extended State Management
#define ECX_AVX            (1 << 28) // AVX Instructions
#define ECX_F16C           (1 << 29) // 16-bit Floating Point Instructions
#define ECX_RDRND          (1 << 30) // RDRAND Instruction
#define ECX_HYPER          (1 << 31) // Hypervisor Present

#define EDX_FPU            (1 << 0)  // Floating-Point Unit On-Chip
#define EDX_VME            (1 << 1)  // Virtual 8086 Mode Extensions
#define EDX_DE             (1 << 2)  // Debugging Extensions
#define EDX_PSE            (1 << 3)  // Page Size Extension
#define EDX_TSC            (1 << 4)  // Time Stamp Counter
#define EDX_MSR            (1 << 5)  // Model Specific Registers
#define EDX_PAE            (1 << 6)  // Physical Address Extension
#define EDX_MCE            (1 << 7)  // Machine-Check Exception
#define EDX_CX8            (1 << 8)  // CMPXCHG8 Instruction
#define EDX_APIC           (1 << 9)  // APIC On-Chip
#define EDX_SEP            (1 << 11) // SYSENTER/SYSEXIT instructions
#define EDX_MTRR           (1 << 12) // Memory Type Range Registers
#define EDX_PGE            (1 << 13) // Page Global Bit
#define EDX_MCA            (1 << 14) // Machine-Check Architecture
#define EDX_CMOV           (1 << 15) // Conditional Move Instruction
#define EDX_PAT            (1 << 16) // Page Attribute Table
#define EDX_PSE36          (1 << 17) // 36-bit Page Size Extension
#define EDX_PSN            (1 << 18) // Processor Serial Number
#define EDX_CLFSH          (1 << 19) // CLFLUSH Instruction
#define EDX_DS             (1 << 21) // Debug Store
#define EDX_ACPI           (1 << 22) // Thermal Monitor and Software Clock Facilities
#define EDX_MMX            (1 << 23) // MMX Technology
#define EDX_FXSR           (1 << 24) // FXSAVE and FXSTOR Instructions
#define EDX_SSE            (1 << 25) // Streaming SIMD Extensions
#define EDX_SSE2           (1 << 26) // Streaming SIMD Extensions 2
#define EDX_SS             (1 << 27) // Self Snoop
#define EDX_HTT            (1 << 28) // Multi-Threading
#define EDX_TM             (1 << 29) // Thermal Monitor
#define EDX_IA64           (1 << 30) // IA64 Processor emulating x86
#define EDX_PBE            (1 << 31) // Pending Break Enable

// Extended Function 0x01
#define EDX_SYSCALL        (1 << 11) // SYSCALL/SYSRET
#define EDX_XD             (1 << 20) // Execute Disable Bit
#define EDX_1GB_PAGE       (1 << 26) // 1 GB Pages
#define EDX_RDTSCP         (1 << 27) // RDTSCP and IA32_TSC_AUX
#define EDX_64_BIT         (1 << 29) // 64-bit Architecture

static inline void cpuid(uint32_t reg, uint32_t *eax, uint32_t *ebx, uint32_t *ecx, uint32_t *edx)
{
    __asm__ volatile("cpuid"
        : "=a" (*eax), "=b" (*ebx), "=c" (*ecx), "=d" (*edx)
        : "0" (reg));
}

uint32_t cpuinfo_func( fs_node_t* node __attribute__((unused)), uint32_t offset, uint32_t size, uint8_t* buffer )
{
    char* buf = calloc(1024, sizeof(char));
    // Register storage
    uint32_t eax, ebx, ecx, edx;

    // Function 0x00 - Vendor-ID and Largest Standard Function

    uint32_t largestStandardFunc;
    char vendor[13];
    cpuid(0, &largestStandardFunc, (uint32_t *)(vendor + 0), (uint32_t *)(vendor + 8), (uint32_t *)(vendor + 4));
    vendor[12] = '\0';

    strcat(buf, "CPU Vendor: ");
    strcat(buf, vendor);

    // Function 0x01 - Feature Information

    if (largestStandardFunc >= 0x01)
    {
        cpuid(0x01, &eax, &ebx, &ecx, &edx);

        //ConsolePrint("Features:");
        strcat(buf, "\nFeatures:");

        if( edx & EDX_FPU )     strcat(buf, " FPU");
        if( edx & EDX_VME )     strcat(buf, " VME");
        if( edx & EDX_DE )      strcat(buf, " DE");
        if( edx & EDX_PSE )     strcat(buf, " PSE");
        if( edx & EDX_TSC )     strcat(buf, " TSC");
        if( edx & EDX_MSR )     strcat(buf, " MSR");
        if( edx & EDX_PAE )     strcat(buf, " PAE");
        if( edx & EDX_MCE )     strcat(buf, " MCE");
        if( edx & EDX_CX8 )     strcat(buf, " CX8");
        if( edx & EDX_APIC )    strcat(buf, " APIC");
        if( edx & EDX_SEP )     strcat(buf, " SEP");
        if( edx & EDX_MTRR )    strcat(buf, " MTRR");
        if( edx & EDX_PGE )     strcat(buf, " PGE");
        if( edx & EDX_MCA )     strcat(buf, " MCA");
        if( edx & EDX_CMOV )    strcat(buf, " CMOV");
        if( edx & EDX_PAT )     strcat(buf, " PAT");
        if( edx & EDX_PSE36 )   strcat(buf, " PSE36");
        if( edx & EDX_PSN )     strcat(buf, " PSN");
        if( edx & EDX_CLFSH )   strcat(buf, " CLFSH");
        if( edx & EDX_DS )      strcat(buf, " DS");
        if( edx & EDX_ACPI )    strcat(buf, " ACPI");
        if( edx & EDX_MMX )     strcat(buf, " MMX");
        if( edx & EDX_FXSR )    strcat(buf, " FXSR");
        if( edx & EDX_SSE )     strcat(buf, " SSE");
        if( edx & EDX_SSE2 )    strcat(buf, " SSE2");
        if( edx & EDX_SS )      strcat(buf, " SS");
        if( edx & EDX_HTT )     strcat(buf, " HTT");
        if( edx & EDX_TM )      strcat(buf, " TM");
        if( edx & EDX_IA64 )    strcat(buf, " IA64");
        if( edx & EDX_PBE )     strcat(buf, " PBE");

        if( ecx & ECX_SSE3 )    strcat(buf, " SSE3");
        if( ecx & ECX_PCLMULQDQ ) strcat(buf, " PCLMULQDQ");
        if( ecx & ECX_DTES64 )  strcat(buf, " DTES64");
        if( ecx & ECX_MONITOR ) strcat(buf, " MONITOR");
        if( ecx & ECX_DS_CPL )  strcat(buf, " DS_CPL");
        if( ecx & ECX_VMX )     strcat(buf, " VMX");
        if( ecx & ECX_SMX )     strcat(buf, " SMX");
        if( ecx & ECX_EST )     strcat(buf, " EST");
        if( ecx & ECX_TM2 )     strcat(buf, " TM2");
        if( ecx & ECX_SSSE3 )   strcat(buf, " SSSE3");
        if( ecx & ECX_CNXT_ID ) strcat(buf, " CNXD_ID");
        if( ecx & ECX_SDBG )    strcat(buf, " SDBG");
        if( ecx & ECX_FMA )     strcat(buf, " FMA");
        if( ecx & ECX_CX16 )    strcat(buf, " CX16");
        if( ecx & ECX_XTPR )    strcat(buf, " XTPR");
        if( ecx & ECX_PDCM )    strcat(buf, " PDCM");
        if( ecx & ECX_PCID )    strcat(buf, " PCID");
        if( ecx & ECX_DCA )     strcat(buf, " DCA");
        if( ecx & ECX_SSE41 )   strcat(buf, " SSE41");
        if( ecx & ECX_SSE42 )   strcat(buf, " SSE42");
        if( ecx & ECX_X2APIC )  strcat(buf, " X2APIC");
        if( ecx & ECX_MOVBE )   strcat(buf, " MOVBE");
        if( ecx & ECX_POPCNT )  strcat(buf, " POPCNT");
        if( ecx & ECX_TSC )     strcat(buf, " TSC-DEADLINE");
        if( ecx & ECX_AES )     strcat(buf, " AES");
        if( ecx & ECX_XSAVE )   strcat(buf, " XSAVE");
        if( ecx & ECX_OSXSAVE)  strcat(buf, " OSXSAVE");
        if( ecx & ECX_AVX )     strcat(buf, " AVX");
        if( ecx & ECX_F16C )    strcat(buf, " F16C");
        if( ecx & ECX_RDRND )   strcat(buf, " RDRND");
        if( ecx & ECX_HYPER )   strcat(buf, " HYPERVISOR");
    }

    // Extended Function 0x00 - Largest Extended Function

    uint32_t largestExtendedFunc;
    cpuid(0x80000000, &largestExtendedFunc, &ebx, &ecx, &edx);

    // Extended Function 0x01 - Extended Feature Bits

    if (largestExtendedFunc >= 0x80000001)
    {
        cpuid(0x80000001, &eax, &ebx, &ecx, &edx);

        if (edx & EDX_64_BIT)
        {
            //ConsolePrint("64-bit Architecture\n");
            strcat(buf, "\nArch: x86_64");
        }else
        {
            strcat(buf, "\nArch: x86");
        }
    }

    // Extended Function 0x02-0x04 - Processor Name / Brand String

    if (largestExtendedFunc >= 0x80000004)
    {
        char name[48];
        cpuid(0x80000002, (uint32_t *)(name +  0), (uint32_t *)(name +  4), (uint32_t *)(name +  8), (uint32_t *)(name + 12));
        cpuid(0x80000003, (uint32_t *)(name + 16), (uint32_t *)(name + 20), (uint32_t *)(name + 24), (uint32_t *)(name + 28));
        cpuid(0x80000004, (uint32_t *)(name + 32), (uint32_t *)(name + 36), (uint32_t *)(name + 40), (uint32_t *)(name + 44));

        // Processor name is right justified with leading spaces
        const char *p = name;
        while (*p == ' ')
        {
            ++p;
        }

        //ConsolePrint("CPU Name: %s\n", p);
        strcat(buf, "\nCPU Name: ");
        strcat(buf, p);
    }
    strcat(buf, "\n");

    size_t _bsize = strlen(buf);
    if( offset > _bsize ) return 0;
    if( size > _bsize - offset ) size = _bsize - offset;

    memcpy(buffer, buf + offset, size);
    return size;
}
