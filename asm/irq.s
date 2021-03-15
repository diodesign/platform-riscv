# hypervisor low-level interrupt/exception code for RV64G targets
#
# Note: No support for F/D floating point (yet)!
#
# (c) Chris Williams, 2019-2020.
# See LICENSE for usage and copying.

.altmacro

.section .text
.align 8

.global irq_early_init

# hypervisor constants, such as stack and lock locations
.include "src/platform-riscv/asm/consts.s"

# set up boot interrupt handling on this core so we can catch
# exceptions while the system is initializating.
# also enable hardware interrupts.
# <= corrupts t0
irq_early_init:
  # point core at default machine-level exception/interrupt handler
  la    t0, machine_irq_handler
  csrrw x0, mtvec, t0

  # skip delegations if there's no supervisor mode to delegate to
  csrrs     t0, misa, x0
  li        t1, 1 << 18     # bit 18 set in misa = S mode present
  and       t0, t0, t1      # if it's not set, no S mode, so park the core
  beq       x0, t0, irq_early_init_post_delegation

  # delegate most exceptions to the supervisor capsule
  # so that it can deal with them direct. for a given exception,
  # bit = 1 to delegate, 0 = pass to the machine-level hypervisor 
  # 0xb1f3 = delegate all exceptions (0-15) except:
  # 02: illegal instruction (catch in case we need to implement a feature)
  # 03: breakpoint
  # 09: environment call from supervisor mode
  # 10: reserved
  # 11: environment call from machine mode
  # 14: reserved
  li    t0, 0xb1f3
  csrrw x0, medeleg, t0

  # 0x333 = delegate the following to their modes:
  # bit 0: User software interrupt
  # bit 1: Supervisor software interrupt
  # bit 4: User timer interrupt
  # bit 5: Supervisor timer interrupt
  # bit 8: User external interrupt
  # bit 9: Supervisor external interrupt
  li    t0, 0x333
  csrrw x0, mideleg, t0

irq_early_init_post_delegation:
  # enable all interrupts: set bit 3 in mstatus to enable machine irqs (MIE)
  # to receive hardware interrupts and exceptions
  csrrsi x0, mstatus, 1 << 3
  ret

# macro to generate store instructions to push given 'reg' register
.macro PUSH_REG reg
  # sw  x\reg, (\reg * 4)(sp) # RV32
  sd  x\reg, (\reg * 8)(sp)
.endm

# macro to generate load instructions to pull given 'reg' register
.macro PULL_REG reg
  # lw  x\reg, (\reg * 4)(sp) # RV32
  ld  x\reg, (\reg * 8)(sp)
.endm


.align 8
# Entry point for machine-level handler of interrupts and exceptions
# interrupts are automatically disabled on entry.
# right now, IRQs are non-reentrant. if an IRQ handler is interrupted, the previous one will
# be discarded. do not enable hardware interrupts. any exceptions will be unfortunate.
machine_irq_handler:
  # get exception handler stack from mscratch by swapping it for interrupted code's sp
  # the handler stack descends from mscratch, the per-CPU variables ascend from it
  csrrw  sp, mscratch, sp
  # now: sp = top of IRQ stack. mscratch = interrupted code's sp

  # reserve space to preserve all 32 GP registers
  addi  sp, sp, -(IRQ_REGISTER_FRAME_SIZE)
  # skip x0 (zero) and x2 (sp), stack all other registers
  PUSH_REG 1
  .set reg, 3
  .rept 29
    PUSH_REG %reg
    .set reg, reg + 1
  .endr

  # stack the interrupted code's sp as x2 (sp) in register block
  csrrs t0, mscratch, x0
  # sw  t0, (2 * 4)(sp) # RV32
  sd    t0, (2 * 8)(sp)

  # right now mscratch is corrupt with the interrupted code's sp.
  # this means hypervisor functions relying on mscratch will break, so restore it.
  addi  t0, sp, IRQ_REGISTER_FRAME_SIZE
  csrrw x0, mscratch, t0

  # for syscalls, riscv sets epc to the address of the syscall instruction.
  # in which case, we need to advance epc 4 bytes to the next instruction.
  # (all instructions are 4 bytes long, for RV32 and RV64)
  # otherwise, we're going into a loop when we return. do this now because the syscall
  # could schedule in another context, so incrementing epc after hypervisor_irq_handler
  # may break a newly scheduled context. we increment mepc directly so that if another
  # context isn't scheduled in, epc will be correct.
  csrrs t0, mcause, x0
  li    t1, 9             # mcause = 9 for environment call from supervisor-to-hypervisor
  bne   t0, t1, continue  # ... all usermode ecalls are handled at the supervisor level
  csrrs t2, mepc, x0      # ... and the hypervisor doesn't make ecalls into itself
  addi  t2, t2, 4
  csrrw x0, mepc, t2

continue:
  # pass current sp to exception/hw handler as a pointer. this'll allow
  # the higher-level hypervisor access and modify any of the stacked registers
  add   a0, sp, x0
  call  hypervisor_irq_handler

  # restore all stacked registers, skipping zero (x0) and sp (x2)
  .set reg, 31
  .rept 29
    PULL_REG %reg
    .set reg, reg - 1
  .endr
  PULL_REG 1

  # finally, restore the interrupted code's sp and return
  PULL_REG 2
  mret


.align 8
# use this interrupt handler to debug failures during system bring-up
# on boards that use SiFive's UART serial controller.
# this doesn't return, so it's not for normal use. it outputs:
#
# I SoC CPU core ID
# C Cause of the trap
# E Address of instruction at time of trap
# A Memory address that triggered a bad access trap
# R Return address register at time of trap
# S Stack pointer at time of trap
#
sifive_guru_mediation:
  mv    t6, ra

  call print_newline
  csrrs t0, mhartid, x0
  li    t1, 'I'
  call print_hex
  csrrs t0, mcause, x0
  li    t1, 'C'
  call print_hex
  csrrs t0, mepc, x0
  li    t1, 'E'
  call print_hex
  csrrs t0, mtval, x0
  li    t1, 'A'
  call print_hex
  mv    t0, t6
  li    t1, 'R'
  call print_hex
  mv    t0, sp
  li    t1, 'S'
  call print_hex

halt:
  j halt

# t0 = value to write, t1 = one character label, scratches t1, t2, t3
print_hex:
  addi  sp, sp, -8
  sd    ra, (sp)
  call  print_char
  li    t1, ' '
  call  print_char
  la    t3, chars
  li    t2, 64
print_hex_loop:
  addi  t2, t2, -4
  srl   t1, t0, t2
  andi  t1, t1, 0xf
  add   t1, t1, t3
  lb    t1, (t1)
  call  print_char
  bne   x0, t2, print_hex_loop
  call  print_newline
  ld    ra, (sp)
  addi  sp, sp, 8
  ret

print_newline:
  addi  sp, sp, -8
  sd    ra, (sp)
  li    t1, '\r'
  call  print_char
  li    t1, '\n'
  call  print_char
  ld    ra, (sp)
  addi  sp, sp, 8
  ret

# t1 = character to write, scratches t4, t5
print_char:
  li    t4, 0x10010000
  lw    t4, (t4)
  bne   x0, t4, print_char
  li    t4, 0x10010000
  sw    t1, (t4)
  ret

chars:
.ascii "0123456789abcdef"