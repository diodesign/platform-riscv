# diosix RV64G hypervisor low-level per-CPU core timer control
#
# (c) Chris Williams, 2019.
#
# See LICENSE for usage and copying.

.altmacro

.section .text
.align 8

.global platform_timer_target
.global platform_timer_get_target
.global platform_timer_now
.global platform_timer_machine_enable
.global platform_timer_supervisor_enable
.global platform_timer_supervisor_trigger
.global platform_timer_supervisor_clear

# hypervisor constants, such as stack and lock locations
.include "src/platform-riscv/asm/consts.s"

# special memory mapped registers for controlling per-CPU timers.
# these addresses are relative from the CLINT MMIO base address,
# which must be passed by the caller.
#
# when the value at mtimecmp > mtime then an IRQ is raised
# this is used to drive the scheduling system  
# mtime is in a single location. each core has its own mtimecmp
# at mtimecmp + hartid * 8
.equ mtimecmp,    0x4000
.equ mtime,       0xbff8

# set the per-CPU timer trigger value. when the timer value >= target, IRQ is raised
# trigger values are stored one 64-bit word per CPU core starting from mtimecmp
# => on RV32: (a0, a1) = trigger on this 64-bit timer value 
#             a2 = 32-bit CLINT IO controller base address
#    on RV64: a0 = trigger on this 64-bit timer value 
#             a1 = 64-bit CLINT IO controller base address
platform_timer_target:
  li      t1, mtimecmp      # get base address of time compare register bank
  csrrc   t2, mhartid, x0   # t2 = heartid
  slli    t2, t2, 3         # t2 = hartid * 8 bytes (hartid * one 64-bit word)
  add     t1, t1, t2        # t1 = mtimecmp + hartid * 8 = address of this CPU's mtimecmp

  # for RV32 targets only
  # add   t1, t1, a2        # get final address of mtimecmp from CLINT base address in a2
  # li    t0, -1            # for RV32, manuals recommend setting all high bits first
  # sw    t0, 4(t1)
  # sw    a0, 0(t1)         # then write low 32-bit word
  # sw    a1, 4(t1)         # then the high 32-bit word

  add     t1, t1, a1        # get final address of mtimecmp from CLINT base address in a1
  sd      a0, 0(t1)         # 64-bit CPUs can just do a single write

  ret

# read the 64-bit per-CPU timer trigger value
# => on RV32: a0 = 32-bit CLINT IO controller base address
#    on RV64: a0 = 64-bit CLINT IO controller base address
# <= on RV32: a0, a1 = trigger on this 64-bit timer value
#    on RV63: a0 = trigger on this 64-bit timer value 
platform_timer_get_target:
  li      t1, mtimecmp      # get base address of time compare register bank
  csrrc   t2, mhartid, x0   # t2 = heartid
  slli    t2, t2, 3         # t2 = hartid * 8 bytes (hartid * one 64-bit word)
  add     t1, t1, t2        # t1 = mtimecmp + hartid * 8 = address of this CPU's mtimecmp

  # for RV32 targets only
  # add   t1, t1, a0        # get final address of mtimecmp from CLINT base address in a0
  # lw    a0, 0(t1)         # read the low 32-bit word
  # lw    a1, 4(t1)         # read the high 32-bit word

  add     t1, t1, a0        # get final address of mtimecmp from CLINT base address in a0
  ld      a0, 0(t1)         # 64-bit CPUs can do a single 64-bit read

  ret

# return the CPU timer's latest value
# => a0 = 32 or 64-bit CLINT IO controller base address
# <= on RV32: a0, a1 = 64-bit value of timer register
#    on RV64: a0 = 64-bit value of timer register
platform_timer_now:
  li  t0, mtime

  # for RV32 targets only
  # add t0, t0, a0                  # get final address of mtime from CLINT base address in a0
  # lw  a1, 4(t0)                   # 32-bit CPUs have to read hi then lo
  # lw  a0, 0(t0)
  # lw  t1, 4(t0)                   # re-read the high word again
  # bne a1, t1, platform_timer_now  # try again if a high-word rollover occurred

  add t0, t0, a0                  # get final address of mtime from CLINT base address in a0
  ld  a0, 0(t0)                   # 64-bit CPUs can just read a whole double word

  ret

# enable the machine-level per-CPU incremental timer
platform_timer_machine_enable:
  li      t0, 1 << 7    # bit 7 = machine timer enable
  csrrs   x0, mie, t0
  ret

# enable timer interrupts for supervisor
platform_timer_supervisor_enable:
  li      t0, 1 << 5    # bit 5 = supervisor timer enable
  csrrs   x0, mie, t0
  ret

# trigger a supervisor timer interrupt by hand
platform_timer_supervisor_trigger:
  li      t0, 1 << 5    # bit 5 = supervisor timer pending, set to raise irq
  csrrs   x0, mip, t0
  ret

# clear the supervisor timer pending bit
platform_timer_supervisor_clear:
  li      t0, 1 << 5    # bit 5 = supervisor timer pending, clear to end irq
  csrrc   x0, mip, t0
  ret