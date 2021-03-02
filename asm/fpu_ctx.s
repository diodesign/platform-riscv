# hypervisor FPU context switch code for RV64G targets
#
# (c) Chris Williams, 2021.
# See LICENSE for usage and copying.

.altmacro

.section .text
.align 8

.global platform_save_supervisor_fp32_state
.global platform_save_supervisor_fp64_state
.global platform_load_supervisor_fp32_state
.global platform_load_supervisor_fp64_state

# hypervisor constants, such as stack and lock locations
.include "src/platform-riscv/asm/consts.s"

# macro to generate store instructions to push given 32-bit fp 'reg' register
.macro PUSH_REG_32 reg
.if fpwidth >= 32
  fsw  f\reg, (\reg * 4)(a0)
.endif
.endm

# macro to generate load instructions to pull given 32-bit fp 'reg' register
.macro PULL_REG_32 reg
.if fpwidth >= 32
  flw  f\reg, (\reg * 4)(a0)
.endif
.endm

# macro to generate store instructions to push given 64-bit fp 'reg' register
.macro PUSH_REG_64 reg
.if fpwidth >= 64
  fsd  f\reg, (\reg * 8)(a0)
.endif
.endm

# macro to generate load instructions to pull given 64-bit fp 'reg' register
.macro PULL_REG_64 reg
.if fpwidth >= 64
  fld  f\reg, (\reg * 8)(a0)
.endif
.endm

# copy 32-bit floating-point registers to memory
# a0 = pointer to array to hold fp register file
platform_save_supervisor_fp32_state:
  .set reg, 0
  .rept 32
    PUSH_REG_32 %reg
    .set reg, reg + 1
  .endr
  ret

# load 32-bit floating-point registers from memory
# a0 = pointer to array of fp registers to write to register file
platform_load_supervisor_fp32_state:
  .set reg, 0
  .rept 32
    PULL_REG_32 %reg
    .set reg, reg + 1
  .endr
  ret

# copy 64-bit floating-point registers to memory
# a0 = pointer to array to hold fp register file
platform_save_supervisor_fp64_state:
  .set reg, 0
  .rept 32
    PUSH_REG_64 %reg
    .set reg, reg + 1
  .endr
  ret

# load 64-bit floating-point registers from memory
# a0 = pointer to array of fp registers to write to register file
platform_load_supervisor_fp64_state:
  .set reg, 0
  .rept 32
    PULL_REG_64 %reg
    .set reg, reg + 1
  .endr
  ret
