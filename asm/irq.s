# hypervisor low-level interrupt/exception code for RV32G/RV64G targets
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

  # delegate most exceptions to the supervisor guest kernel
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

  # enable all interrupts: set bit 3 in mstatus to enable machine irqs (MIE)
  # to receive hardware interrupts and exceptions
  li    t0, 1 << 3
  csrrs x0, mstatus, t0
  ret

# macro to generate store instructions to push given 'reg' register
.macro PUSH_REG reg
.if ptrwidth == 32
  sw  x\reg, (\reg * 4)(sp)
.else
  sd  x\reg, (\reg * 8)(sp)
.endif
.endm

# macro to generate load instructions to pull given 'reg' register
.macro PULL_REG reg
.if ptrwidth == 32
  lw  x\reg, (\reg * 4)(sp)
.else
  ld  x\reg, (\reg * 8)(sp)
.endif
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
.if ptrwidth == 32
  sw    t0, (2 * 4)(sp)
.else
  sd    t0, (2 * 8)(sp)
.endif

  # right now mscratch is corrupt with the interrupted code's sp.
  # this means hypervisor functions relying on mscratch will break, so restore it.
  addi  t0, sp, IRQ_REGISTER_FRAME_SIZE
  csrrw x0, mscratch, t0

  # for syscalls, riscv sets epc to the address of the syscall instruction.
  # in which case, we need to advance epc 4 bytes to the next instruction.
  # (all instructions are 4 bytes long, for RV32 and RV64)
  # otherwise, we're going into a loop when we return. do this now because the syscall
  # could schedule in another context, so incrementing epc after kirq_handler
  # may break a newly scheduled context. we increment mepc directly so that if another
  # context isn't scheduled in, epc will be correct.
  csrrs t0, mcause, x0
  csrrs t1, mepc, x0
  li    t2, 9             # mcause = 9 for environment call from supervisor-to-hypervisor
  bne   t0, t2, continue  # ... all usermode ecalls are handled at the supervisor level
  addi  t1, t1, 4         # ... and the hypervisor doesn't make ecalls into itself
  csrrw x0, mepc, t1

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
