# hypervisor low-level utility code for RV64G targets
#
# (c) Chris Williams, 2019-2021.
# See LICENSE for usage and copying.

.section .text
.align 8

.global platform_cpu_private_variables
.global platform_cpu_heap_base
.global platform_cpu_heap_size
.global platform_set_supervisor_return
.global platform_read_u32_as_prev_mode

# hypervisor constants, such as stack and lock locations
.include "src/platform-riscv/asm/consts.s"

# return pointer to this CPU's private variables
# <= a0 = pointer to hypervisor's CPU structure
platform_cpu_private_variables:
  # get base of private variables from top of IRQ stack, held in mscratch
  csrrs a0, mscratch, x0
  ret

# return base address of this CPU's heap - right above private vars 
# <= a0 = pointer to heap base (corrupts t0)
platform_cpu_heap_base:
  csrrs a0, mscratch, x0  # private vars start above CPU IRQ stack
  li    t0, HV_CPU_PRIVATE_VARS_SIZE
  add   a0, a0, t0
  ret

# return total empty size of this CPU's heap area
# <= a0 = heap size in bytes
platform_cpu_heap_size:
  li  a0, HV_CPU_HEAP_AREA_SIZE
  ret

# set the machine-level flags necessary to return to supervisor mode
# rather than machine mode. context for the supervisor mode is loaded
# elsewhere
platform_set_supervisor_return:
  # set 'previous' privilege level to supervisor by clearing bit 12
  # and setting bit 11 in mstatus, defining MPP[12:11] as b01 = 1 for supervisor
  li    t0, 1 << 12
  csrrc x0, mstatus, t0
  li    t0, 1 << 11
  csrrs x0, mstatus, t0
  ret

# read a u32 from memory as the previous privilege mode. if this read fails,
# it will generate a permisison or access fault as the previous privilege mode
# => a0 = address to read
# <= a0 = u32 read in
platform_read_u32_as_prev_mode:
  csrrs t1, mstatus, x0           # keep a copy of mstatus in case the read faults
  la    t2, trap_read_u32_fault   # t2 = address of our temporary fault catcher
  csrrw t2, mtvec, t2             # swap t2 and original fault handler

  li    t0, (1 << 17) | (1 << 19) # set bits 17 (MPRV) and 19 (MXR) to read as previous mode
  csrrs x0, mstatus, t0
  lw    a0, (a0)                  # do the read
  csrrc x0, mstatus, t0           # clear those MPRV and MXR bits

  csrrw t2, mtvec, t2             # restore the original fault handler
  ret

# pin the blame on the previous mode (in mstatus.mpp) by swapping in previous mstatus
# this means the fault will appear from that mode
trap_read_u32_fault:
  csrrw x0, mstatus, t1           # restore the mstatus with the previous mpp, and no MPRV + MXR set
  csrrw x0, mtvec, t2             # restore the original fault handler
  jalr  x0, t2                    # jump to fault handler to deal with error
