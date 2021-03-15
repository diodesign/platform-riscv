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
  csrrs t5, mstatus, x0           # t5 = mstatus in case the read faults
  la    t6, trap_read_u32_fault   # t6 = address of our temporary fault catcher
  csrrw t6, mtvec, t6             # swap t6 and original fault handler

  li    t0, (1 << 17) | (1 << 19) # set bits 17 (MPRV) and 19 (MXR) to read as previous mode
  csrrs x0, mstatus, t0

  # some hardware can't do unaligned reads, so if we're not 32-bit aligned
  # read the instruction byte by byte instead
  andi  t0, a0, 3
  beq   x0, t0, platform_read_u32_as_prev_mode_aligned
  
  # read the unaligned instruction a byte at a time
  mv    t0, x0                    # where we'll store our fetched instruction
  li    t1, 4                     # loop counter: copy 4 bytes
platform_read_u32_as_prev_mode_byte_loop:
  slli  t0, t0, 8                 # shift u32 register up to make space for next byte
  addi  t1, t1, -1                # decrement loop counter now so we copy bytes 3 to 0 inclusive
  add   t2, t1, a0                # compute address of byte to fetch
  lbu   t2, (t2)                  # read in the byte without sign extending
  or    t0, t0, t2                # paste the byte's bits onto low byte of u32 register
  bne   x0, t1, platform_read_u32_as_prev_mode_byte_loop
  mv    a0, t0                    # fetched 32-bit word expected in a0

  # sign extend a0 if necessary to keep rust happy
  # TODO: not sure why this is neccesary. some FFI issue?
  li    t0, 1 << 31
  and   t0, t0, a0
  beq   x0, t0, platform_read_u32_as_prev_mode_done
  li    t0, 0xffffffff00000000
  or    a0, a0, t0
  j     platform_read_u32_as_prev_mode_done

platform_read_u32_as_prev_mode_aligned:
  lw   a0, (a0)                   # do the 32-bit aligned read into a0

platform_read_u32_as_prev_mode_done:
  csrrw x0, mstatus, t5           # restore previous status
  csrrw x0, mtvec, t6             # restore the original fault handler
  ret

# pin the blame on the previous mode (in mstatus.mpp) by swapping in previous mstatus
# this means the fault will appear from that mode
trap_read_u32_fault:
  csrrw x0, mstatus, t5           # restore the mstatus with the previous mpp, and no MPRV + MXR set
  csrrw x0, mtvec, t6             # restore the original fault handler
  jalr  x0, t6                    # jump to fault handler to deal with error
