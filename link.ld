/* diosix ELF layout for common RV64 hardware
 *
 * Only requirement is that physical RAM starts at 0x80000000, where the hypervisor is loaded and entered
 * FIXME: whnat happens when the physical RAM is not at 0x80000000?
 *
 * Do a cargo clean after modifying this file.
 *
 * (c) Chris Williams, 2019-2020.
 * See LICENSE for usage and copying.
 */

OUTPUT_ARCH(riscv)
ENTRY(_start)

SECTIONS
{
  /* execution starts at 0x80000000, DRAM base */
  . = 0x80000000;
  __hypervisor_start = .;

  .text : ALIGN(0x8)
  {
    *(.entry) /* must be first */
    *(.text*)
    *(.rodata*)
  }

  .data : ALIGN(0x8)
  {
    *(.data*)
  }

  /* this needs to be zeroed */
  __bss_start = ALIGN(0x8);
  .bss : ALIGN(0x8)
  {
    *(.*bss*)
  }
  __bss_end = ALIGN(0x8);

  /* set aligned end to the hypervisor, round up to nearest page boundary */
  __hypervisor_end = ALIGN(0x1000);
}
