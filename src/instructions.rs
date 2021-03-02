/* diosix RISC-V instruction emulation
 *
 * Implement instructions missing from the underlying hardware
 * 
 * (c) Chris Williams, 2020.
 *
 * See LICENSE for usage and copying.
 */

use super::irq::IRQContext;
use super::cpu::PrivilegeMode;
use super::timer;

extern "C"
{
    fn platform_read_u32_as_prev_mode(address: usize) -> u32;
}

#[derive(PartialEq)]
pub enum EmulationResult
{
    Success, /* we were able to emulate the faulting instruction */
    CantEmulate, /* don't have the means to emulate this instruction */
    CantAccess, /* can't locate or access the illegal instruction */
    IllegalInstruction, /* this instruction is truly illegal, can't be run */
    Yield /* this supervisor is yielding to other guests */
}

/* instructions we can handle here */
const RDTIME_INST:  u32 = 0xc01 << 20 | 2 << 12 | 0x1c << 2 | 3;
const RDTIME_MASK:  u32 = !(0x1f << 7);
const WFI_INST:     u32 = 0x10500073;

/* attempt to emulate the currently faulting instruction. this can use and modify
   the given context as necessary. this function may raise a fault,
   which the hypervisor should catch and deal with appropriately
   => priv_mode = privilege mode the instruction was executed in
      context = state of the CPU core trying to run the instruction,
                which may be modified as necessary.
   <= returns confirmation of emulation, if possible, or not */
pub fn emulate(_priv_mode: PrivilegeMode, context: &mut IRQContext) -> EmulationResult
{
    /* get the address of the faulting instruction */
    let addr = read_csr!(mepc) as usize;

    /* ensure any faults are blamed on the mode that tried to execute the instruction */
    let instruction = unsafe { platform_read_u32_as_prev_mode(addr) };

    /* try to enulate the rdtime instruction, which reads the 64-bit real-time clock */
    if (instruction & RDTIME_MASK) == RDTIME_INST
    {
        let time_now = match (timer::get_pinned_timer_now(), timer::get_pinned_timer_freq())
        {
            (Some(t), Some(f)) => t.to_exact(f),
            (_, _) => return EmulationResult::CantEmulate
        };

        /* update destination register with current (low) word of the timer */
        let rd = ((instruction & !RDTIME_MASK) >> 7) & RDTIME_MASK;
        context.registers[rd as usize] = time_now as usize;

        increment_epc(); /* go to next instuction */
        return EmulationResult::Success;
    }

    /* catch WFI as a yield to other supervisor kernels */
    if instruction == WFI_INST
    {
        /* TODO: actually make the vCPU ait for an interrupt? */
        increment_epc(); /* go to next instuction on return */
        return EmulationResult::Yield;
    }

    /* fall through to a confirmed illegal instruction */
    EmulationResult::IllegalInstruction
}

/* increment epc to the next 32-bit instruction.
   TODO: How fragile is this? Assuming 4-byte instr and
   also relying on mepc being used later on as the interrupted
   program counter */
fn increment_epc()
{
    let epc = read_csr!(mepc);
    write_csr!(mepc, epc + 4);
}