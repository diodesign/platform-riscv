# diosix hypervisor common low-level entry points for RV64I platforms
#
# Assumes on entry: a0 = CPU/Hart ID number, a1 -> device tree
#
# All values are little endian unless otherwise specified
#
# (c) Chris Williams, 2019-2020.
# See LICENSE for usage and copying.

# _start *must* be the first routine in this file
.section .entry
.align 8

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
# => a0 = CPU core ID, aka hart ID, assigned by firmware
#    a1 = pointer to device tree
# <= never returns
_start:
  # for now, only run on CPU cores that support at least supervisor mode
  # some boards have a non-S-mode monitor core. there is high-level support
  # for this though low-level work is needed to ensure these cores work properly
  csrrs     t0, misa, x0
  li        t1, 1 << 18     # bit 18 set in misa = S mode present
  and       t0, t0, t1      # if it's not set, no S mode, so park the core
  beq       x0, t0, infinite_loop

  # each core should grab a slab of memory starting from the end of the hypervisor.
  # in order to scale to many cores, not waste too much memory, and to cope with non-linear
  # CPU ID / hart ID, each core will take memory using an atomic counter.
  # thus, memory is allocated on a first come, first served basis.
  la        t1, cpu_core_id_counter
  li        t2, 1
  amoadd.w  t3, t2, (t1)
  mv        a0, t3
  # now a0 = runtime-assigned linear CPU core ID, counting from 0

  # use t3 this as a multiplier from the end of the hypervisor, using shifts to keep things easy
  la        t1, __hypervisor_end
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

  # trap WFI used by supervisors so we can auto-yield to other capsules
  # works only if supported by the hardware platform
  #
  # FIXME: guests run faster if we don't yield? see instructions.rs
  # also Qemu 5.2.50 raises mcause 0x16 with TW set, which causes problems (?)
  #
  # li        t1, 1 << 21 # bit 21 = TW (timewout wait)
  # csrrs     x0, mstatus, t1

  # boot CPU core (ID 0) needs to zero the BSS */
  la        t0, clear_bss_finished
  beq       x0, a0, clear_bss

  # other CPU cores need to wait for clear_bss_finished
  # to change from zero to non-zero to indicate the BSS is clear
clear_bss_wait_loop:
  amoswap.w t1, x0, (t0)
  beq       x0, t1, clear_bss_wait_loop
  j         clear_bss_loop_end

clear_bss:
  la        t1, __bss_start
  la        t2, __bss_end
  bgeu      t1, t2, clear_bss_loop_end # avoid empty or malformed bss 
clear_bss_loop:
  sd        x0, (t1)
  addi      t1, t1, 8
  bltu      t1, t2, clear_bss_loop

clear_bss_loop_end:
  li        t1, 1        # set clear_bss_finished to 1 now we're done
  amoswap.w x0, t1, (t0) # t0 = clear_bss_finished

  # call hventry with:
  # a0 = runtime-assigned CPU ID number
  # a1 = pointer to start of devicetree
  # a2 = big-endian length of the devicetree
  lw        a2, 4(a1)       # 32-bit size of tree stored from byte 4 in tree blob
  la        t0, hventry
  jalr      ra, t0, 0

# fall through to loop rather than crash into random instructions/data
infinite_loop:
  wfi
  j         infinite_loop

# variables
.align 8
cpu_core_id_counter:
.word 0

.align 8
clear_bss_finished:
.word 0