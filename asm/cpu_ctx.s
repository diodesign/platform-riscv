# hypervisor CPU context switch code for RV64G targets
#
# (c) Chris Williams, 2021.
# See LICENSE for usage and copying.

.section .text
.align 8

.global platform_save_supervisor_cpu_state
.global platform_load_supervisor_cpu_state

# hypervisor constants, such as stack and lock locations
.include "src/platform-riscv/asm/consts.s"

# save contents of physical CPU's supervisor CSRs and registers
# stacked by IRQ handler into per virtual-core data structure
# => a0 = pointer to SupervisorState structure to hold registers
platform_save_supervisor_cpu_state:
  # preserve all supervisor CSRs
  csrrs t0, sstatus, x0
# csrrs t1, sedeleg, x0   # TODO: needs N extension
# csrrs t2, sideleg, x0   # TODO: needs N extension
  csrrs t3, stvec, x0
  csrrs t4, sip, x0
  csrrs t5, sie, x0
  csrrs t6, scounteren, x0

  sd    t0, 0(a0)     # save 8-byte 64-bit registers
# sd    t1, 8(a0)     # sedeleg needs N extension
# sd    t2, 16(a0)    # sideleg needs N extension
  sd    t3, 24(a0)
  sd    t4, 32(a0)
  sd    t5, 40(a0)
  sd    t6, 48(a0)

  csrrs t0, sscratch, x0
  csrrs t1, sepc, x0
  csrrs t2, scause, x0
  csrrs t3, stval, x0
  csrrs t4, satp, x0
  csrrs t5, mepc, x0    # preserve pc of interrupted code
  csrrs t6, mstatus, x0 # get underlying state of interrupted code

  sd    t0, 56(a0)      # save 64-bit registers
  sd    t1, 64(a0)
  sd    t2, 72(a0)
  sd    t3, 80(a0)
  sd    t4, 88(a0)
  sd    t5, 96(a0)
  sd    t6, 104(a0)

  # copy registers from the IRQ stack
  # addi  t0, a0, 56 # RV32
  addi  t0, a0, 112

  csrrs t1, mscratch, x0
  addi  t1, t1, -(IRQ_REGISTER_FRAME_SIZE)
  # t0 = base of register save block, t1 = base of IRQ saved registers
  # skip over x0
  addi  t1, t1, 8

  # stack remaining 31 registers
  li    t2, 31

from_stack_copy_loop:
  ld    t3, (t1)
  sd    t3, (t0)
  addi  t0, t0, 8
  addi  t1, t1, 8

  addi  t2, t2, -1
  bnez  t2, from_stack_copy_loop

  ret

# load saved supervisor CSRs and general-purpose registers from memory
# to the IRQ stack and physical CPU CSRs so when we return to the
# supervisor, the new context becomes active 
# => a0 = pointer to SupervisorState structure from which to load registers
platform_load_supervisor_cpu_state:
  # restore supervisor CSRs
  ld    t0, 0(a0)
# ld    t1, 8(a0)       # sedeleg needs N extension
# ld    t2, 16(a0)      # sideleg needs N extension
  ld    t3, 24(a0)
  ld    t4, 32(a0)
  ld    t5, 40(a0)
  ld    t6, 48(a0)

  csrrw x0, sstatus, t0
# csrrw x0, sedeleg, t1   # needs N extension
# csrrw x0, sideleg, t2   # needs N extension
  csrrw x0, stvec, t3
  csrrw x0, sip, t4
  csrrw x0, sie, t5
  csrrw x0, scounteren, t6

  ld    t0, 56(a0)
  ld    t1, 64(a0)
  ld    t2, 72(a0)
  ld    t3, 80(a0)
  ld    t4, 88(a0)
  ld    t5, 96(a0)
  ld    t6, 104(a0)

  csrrw x0, sscratch, t0
  csrrw x0, sepc, t1
  csrrw x0, scause, t2
  csrrw x0, stval, t3
  csrrw x0, satp, t4      # change the page table base ptr
  sfence.vma x0, x0       # make sure the MMU picks up the change
  csrrw x0, mepc, t5      # restore pc of next context to run

  # only update selected mstatus bits: mpp (11-12)
  li    t0, (1 << 12) | (1 << 11)
  csrrc x0, mstatus, t0
  and   t6, t6, t0
  csrrs x0, mstatus, t6

  # copy registers to the IRQ stack
  addi  t0, a0, 112

  csrrs t1, mscratch, x0
  addi  t1, t1, -(IRQ_REGISTER_FRAME_SIZE)
  # t0 = base of register save block, t1 = base of IRQ saved registers
  # skip over x0
  addi  t1, t1, 8

  # copy remaining 31 registers
  li    t2, 31

to_stack_copy_loop:
  ld    t3, (t0)
  sd    t3, (t1)
  addi  t0, t0, 8
  addi  t1, t1, 8

  addi  t2, t2, -1
  bnez  t2, to_stack_copy_loop

  ret