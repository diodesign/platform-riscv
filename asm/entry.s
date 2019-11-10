# diosix hypervisor common low-level entry points for RV32G/RV64G platforms
#
# Assumes a0 = CPU/Hart ID number, a1 -> device tree
#
# (c) Chris Williams, 2019.
# See LICENSE for usage and copying.

# _start *must* be the first routine in this file
.section .entry
.align 4

.global _start

# hypervisor constants, such as global variable and lock locations
# check this file for static hypervisor data layout
.include "src/platform-riscv/asm/consts.s"

# typical hardware physical memory map
# 0x00000000, size: 0x100:     Debug ROM/data
# 0x00001000, size: 0x11000:   Boot ROM
# 0x00100000, size: 0x1000:    Hardware test area
# 0x02000000, size: 0x10000:   CLINT (Core Local Interruptor)
# 0x0c000000, size: 0x4000000: PLIC (Platform Level Interrupt Controller)
# 0x80000000: DRAM base <-- hypervisor + entered loaded here
#
# different hardware platforms will place peripherals in different areas.
# check the device tree upon boot for exact phys memory locations
#
# see consts.s for top page of global variables locations and other memory layout decisions

# the boot ROM drops each core simultenously here with nothing setup
# this code is assumed to be loaded and running at 0x80000000
# interrupts and exceptions are disabled.
#
# => a0 = CPU core ID, aka hart ID
#    a1 = pointer to device tree
# <= never returns
_start:
  # each core should grab a slab of memory starting from the end of the hypervisor.
  # in order to scale to many cores, not waste too much memory, and to cope with non-linear
  # CPU ID / hart ID, each core will take memory using an atomic counter in the first word
  # of available RAM. thus, memory is allocated on a first come, first served basis.
  # this counter is temporarily and should be forgotten about once in hvmain()
  la        t1, __hypervisor_end
  li        t2, 1
  amoadd.w  t3, t2, (t1)
  # t3 = counter just before we incremented it
  # preserve t3 in a0
  add       a0, t3, x0
  
  # use t3 this as a multiplier from the end of the hypervisor, using shifts to keep things easy
  slli      t3, t3, HV_CPU_SLAB_SHIFT
  add       t3, t3, t1
  # t3 = base of this CPU's private memory slab

  # write the top of the exception / interrupt stack to mscratch
  li        t1, HV_CPU_STACK_BASE
  li        t2, HV_CPU_STACK_SIZE
  add       t4, t2, t1
  add       t4, t4, t3
  # t4 = top of the stack, t2 = stack size, t1 = stack base from slab base
  csrrw     x0, mscratch, t4

  # use the lower half of the exception stack to bring up the hypervisor
  # set the boot stack pointer to halfway down the IRQ stack
  srli      t1, t2, 1
  sub       sp, t4, t1

  # set up early exception/interrupt handling (corrupts t0)
  # leave hardware interrupts disabled for now
  call      irq_early_init

  # initialize basic settings
  # trap WFI in supervisors so we can auto-yield to other capsules
  li        t1, 1
  slli      t2, t1, 21        # set bit 21 = TW (timewout wait)
  csrrs     x0, mstatus, t2

  # find device tree size to pass to parsing code
  # it's the second 32-bit word in the DTB header
  # beware: this is a big endian value that needs
  # converting to little endian
  lw        t0, 4(a1)
  # swap upper bytes in 32-bit value from BE to LE
  srli      t1, t0, 24
  andi      t1, t1, 0xff    # t1 = byte 3 -> byte 0
  srli      t2, t0, 8
  li        t3, 0xff00
  and       t2, t2, t3      # t2 = byte 2 -> byte 1
  # note: we only support max 64KB device trees
  # ie: we'll only swap over bytes 3 and 2
  # a2 = ((t0 >> 8) & 0xff00) | ((t0 >> 24) & 0xff)
  or        a2, t2, t1

  # call hwentry with runtime-assigned CPU ID number in a0, devicetree in a1, devicetree size in a2
  la        t0, hventry
  jalr      ra, t0, 0

# fall through to loop rather than crash into random instructions/data
# wait for interrupts to come in and service them
infinite_loop:
  wfi
  j         infinite_loop
