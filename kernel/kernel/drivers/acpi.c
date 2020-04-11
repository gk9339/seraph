#include <stddef.h>
#include <string.h>
#include <time.h>
#include <kernel/serial.h>
#include <stdint.h>
#include <kernel/acpi.h>

uint16_t SMI_CommandPort;
uint8_t AcpiEnable;
uint8_t AcpiDisable;
uint16_t PM1aControlBlock;
uint16_t PM1bControlBlock;
uint32_t SLP_TYPa;
uint32_t SLP_TYPb;
uint16_t SLP_EN;
uint16_t SCI_enabled;
uint8_t PM1CotrolLength;

struct RSDPtr
{
    uint8_t Signature[8];
    uint8_t CheckSum;
    uint8_t OemID[6];
    uint8_t Revision;
    uint32_t* RsdtAddress;
};

struct GenericAddressStructure
{
    uint8_t AddressSpace;
    uint8_t BitWidth;
    uint8_t BitOffset;
    uint8_t AccessSize;
    uint64_t Address;
};

struct ACPISDTHeader {
    char Signature[4];
    uint32_t Length;
    uint8_t Revision;
    uint8_t Checksum;
    char OEMID[6];
    char OEMTableID[8];
    uint32_t OEMRevision;
    uint32_t CreatorID;
    uint32_t CreatorRevision;
};

struct FACP
{
    struct   ACPISDTHeader h;
    uint32_t FirmwareCtrl;
    uint32_t* Dsdt;
 
    // field used in ACPI 1.0; no longer in use, for compatibility only
    uint8_t  Reserved;
 
    uint8_t  PreferredPowerManagementProfile;
    uint16_t SCI_Interrupt;
    uint32_t SMI_CommandPort;
    uint8_t  AcpiEnable;
    uint8_t  AcpiDisable;
    uint8_t  S4BIOS_REQ;
    uint8_t  PSTATE_Control;
    uint32_t PM1aEventBlock;
    uint32_t PM1bEventBlock;
    uint32_t PM1aControlBlock;
    uint32_t PM1bControlBlock;
    uint32_t PM2ControlBlock;
    uint32_t PMTimerBlock;
    uint32_t GPE0Block;
    uint32_t GPE1Block;
    uint8_t  PM1EventLength;
    uint8_t  PM1ControlLength;
    uint8_t  PM2ControlLength;
    uint8_t  PMTimerLength;
    uint8_t  GPE0Length;
    uint8_t  GPE1Length;
    uint8_t  GPE1Base;
    uint8_t  CStateControl;
    uint16_t WorstC2Latency;
    uint16_t WorstC3Latency;
    uint16_t FlushSize;
    uint16_t FlushStride;
    uint8_t  DutyOffset;
    uint8_t  DutyWidth;
    uint8_t  DayAlarm;
    uint8_t  MonthAlarm;
    uint8_t  Century;
 
    // reserved in ACPI 1.0; used since ACPI 2.0+
    uint16_t BootArchitectureFlags;
 
    uint8_t  Reserved2;
    uint32_t Flags;
 
    // 12 byte structure; see below for details
    struct GenericAddressStructure ResetReg;
 
    uint8_t ResetValue;
    uint8_t Reserved3[3];
 
    // 64bit pointers - Available on ACPI 2.0+
    uint64_t X_FirmwareControl;
    uint64_t X_Dsdt;
 
    struct GenericAddressStructure X_PM1aEventBlock;
    struct GenericAddressStructure X_PM1bEventBlock;
    struct GenericAddressStructure X_PM1aControlBlock;
    struct GenericAddressStructure X_PM1bControlBlock;
    struct GenericAddressStructure X_PM2ControlBlock;
    struct GenericAddressStructure X_PMTimerBlock;
    struct GenericAddressStructure X_GPE0Block;
    struct GenericAddressStructure X_GPE1Block;
};

// check if the given address has a valid header
static unsigned int *acpi_check_RSD_ptr( uint32_t* ptr )
{
   char* sig = "RSD PTR ";
   struct RSDPtr* rsdp = (struct RSDPtr*)ptr;
   char* bptr;
   uint32_t check = 0;
   uint32_t i;

   if (memcmp(sig, rsdp, 8) == 0)
   {
      // check checksum rsdpd
      bptr = (char*)ptr;
      for( i=0; i<sizeof(struct RSDPtr); i++ )
      {
         check += *bptr;
         bptr++;
      }

      // found valid rsdpd
      if( (uint8_t)check == 0 )
      {
         return (unsigned int *) rsdp->RsdtAddress;
      }
   }

   return NULL;
}

// finds the acpi header and returns the address of the rsdt
static unsigned int* acpi_get_RSD_ptr( void )
{
    unsigned int* addr;
    unsigned int* rsdp;

    // search below the 1mb mark for RSDP signature
    for( addr = (unsigned int *) 0x000E0000; (int) addr<0x00100000; addr += 0x10/sizeof(addr) )
    {
        rsdp = acpi_check_RSD_ptr((uint32_t*)addr);
        if( rsdp != NULL )
        {
            return rsdp;
        }
    }


    // at address 0x40:0x0E is the RM segment of the ebda
    int ebda = *((short *) 0x40E);    // get pointer
    ebda = ebda*0x10 &0x000FFFFF;    // transform segment into linear address

    // search Extended BIOS Data Area for the Root System Description Pointer signature
    for( addr = (unsigned int *) ebda; (int) addr<ebda+1024; addr+= 0x10/sizeof(addr) )
    {
        rsdp = acpi_check_RSD_ptr((uint32_t*)addr);
        if( rsdp != NULL )
        {
            return rsdp;
        }
    }

    return NULL;
}

// checks for a given header and validates checksum
static int acpi_check_header( unsigned int* ptr, char* sig )
{
    if( memcmp(ptr, sig, 4) == 0 )
    {
        uint8_t* checkPtr = (uint8_t*)ptr;
        int len = *(ptr + 1);
        uint32_t check = 0;
        while( 0<len-- )
        {
            check += *checkPtr;
            checkPtr++;
        }
        if( (uint8_t)check == 0 )
        {
            return 0;
        }
    }
    return -1;
}

static int acpiEnable( void )
{
    // check if acpi is enabled
    if( (inportl(PM1aControlBlock) &SCI_enabled) == 0 )
    {
        // check if acpi can be enabled
        if( SMI_CommandPort != 0 && AcpiEnable != 0 )
        {
            outportb(SMI_CommandPort, AcpiEnable); // send acpi enable command
            // give 3 seconds time to enable acpi
            int i;
            for (i=0; i<300; i++ )
            {
                if( (inportl(PM1aControlBlock) &SCI_enabled) == 1 )
                    break;
            }
            if( PM1bControlBlock != 0 )
                for (; i<300; i++ )
                {
                    if( (inportl((unsigned int) PM1bControlBlock) &SCI_enabled) == 1 )
                        break;
                }
            if( i<300 ) 
            {
                debug_log("enabled acpi");
                return 0;
            }else
            {
                debug_log("couldn't enable acpi");
                return -1;
            }
        }else
        {
            debug_log("no known way to enable acpi");
            return -1;
        }
    }else
    {
        debug_log("acpi was already enabled");
        return 0;
    }
}

//
// bytecode of the \_S5 object
// -----------------------------------------
//        | (optional) |    |    |    |    
// NameOP | \          | _  | S  | 5  | _
// 08     | 5A         | 5F | 53 | 35 | 5F
//
// -----------------------------------------------------------------------------------------------------------
//           |           |              | ( SLP_TYPa )   | ( SLP_TYPb )   | ( Reserved )   | ( Reserved )
// PackageOP | PkgLength | NumElements  | byteprefix Num | byteprefix Num | byteprefix Num | byteprefix Num
// 12        | 0A        | 04           | 0A         05  | 0A             05 | 0A            05  | 0A            05
//
//----this-structure-was-also-seen----------------------
// PackageOP | PkgLength | NumElements |
// 12        | 06        | 04          | 00 00 00 00
//
// (Pkglength bit 6-7 encode additional PkgLength bytes [shouldn't be the case here])
//
int initialize_acpi( void )
{
    unsigned int *ptr = acpi_get_RSD_ptr();

    // check if address is correct  ( if acpi is available on this pc )
    if( ptr != NULL && acpi_check_header(ptr, "RSDT") == 0 )
    {
        // the RSDT contains an unknown number of pointers to acpi tables
        int entrys = *(ptr + 1);
        entrys = (entrys-36) /4;
        ptr += 36/4;    // skip header information

        while( 0<entrys-- )
        {
            // check if the desired table is reached
            if( acpi_check_header((unsigned int*) *ptr, "FACP") == 0 )
            {
                entrys = -2;
                struct FACP* facp = (struct FACP*) *ptr;
                if( acpi_check_header((unsigned int*) facp->Dsdt, "DSDT") == 0 )
                {
                    // search the \_S5 package in the DSDT
                    uint8_t* S5Addr = (uint8_t*) facp->Dsdt +36; // skip header
                    int dsdtLength = *(facp->Dsdt+1) -36;
                    while( 0 < dsdtLength-- )
                    {
                        if( memcmp(S5Addr, "_S5_", 4) == 0 )
                            break;
                        S5Addr++;
                    }
                    // check if \_S5 was found
                    if( dsdtLength > 0 )
                    {
                        // check for valid AML structure
                        if( ( *(S5Addr-1) == 0x08 || ( *(S5Addr-2) == 0x08 && *(S5Addr-1) == '\\') ) && *(S5Addr+4) == 0x12 )
                        {
                            S5Addr += 5;
                            S5Addr += ((*S5Addr &0xC0)>>6) +2;    // calculate PkgLength size

                            if(*S5Addr == 0x0A )
                                S5Addr++;    // skip byteprefix
                            SLP_TYPa = *(S5Addr)<<10;
                            S5Addr++;

                            if( *S5Addr == 0x0A )
                                S5Addr++;    // skip byteprefix
                            SLP_TYPb = *(S5Addr)<<10;

                            SMI_CommandPort = (uint16_t)facp->SMI_CommandPort;

                            AcpiEnable = facp->AcpiEnable;
                            AcpiDisable = facp->AcpiDisable;

                            PM1aControlBlock = (uint16_t)facp->PM1aControlBlock;
                            PM1bControlBlock = (uint16_t)facp->PM1bControlBlock;
                            
                            PM1CotrolLength = facp->PM1ControlLength;

                            SLP_EN = 1<<13;
                            SCI_enabled = 1;

                            return 0;
                        }else
                        {
                            debug_log("\\_S5 parse error");
                        }
                    }else
                    {
                        debug_log("\\_S5 not present");
                    }
                }else
                {
                    debug_log("DSDT invalid");
                }
            }
            ptr++;
        }
        debug_log("no valid FACP present");
    }else
    {
        debug_log("no acpi");
    }

    return -1;
}

void acpi_poweroff( void )
{
    // SCI_enabled is set to 1 if acpi poweroff is possible
    if( SCI_enabled == 0 )
        return;

    acpiEnable();

    debug_log("acpi poweroff");
    // send the poweroff command
    outportl(PM1aControlBlock, (uint16_t)(SLP_TYPa | SLP_EN) );
    if( PM1bControlBlock != 0 )
        outportl(PM1bControlBlock, (uint16_t)(SLP_TYPb | SLP_EN) );

    debug_log("acpi poweroff failed");
}
