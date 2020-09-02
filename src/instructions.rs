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
use super::physmem;
use super::timer;
use super::mmu;

#[derive(PartialEq)]
pub enum EmulationResult
{
    Success, /* we were able to emulate the faulting instruction */
    CantEmulate, /* don't have the means to emulate this instruction */
    CantAccess, /* can't locate or access the illegal instruction */
    IllegalInstruction /* this instruction is truly illegal, can't be run */
}

/* instructions we can handle here */
const RDTIME_INST: u32  = 0xc01 << 20 | 2 << 12 | 0x1c << 2 | 3;
const RDTIME_MASK: u32  = !(0x1f << 7);
const RDTIMEH_INST: u32 = 0xc81 << 20 | 2 << 12 | 0x1c << 2 | 3;
const RDTIMEH_MASK: u32 = !(0x1f << 7);

/* attempt to emulate the currently faulting instruction. this can use and modify
   the given context as necessary. this function may raise a fault,
   which the hypervisor should catch and deal with appropriately
   => priv_mode = privilege mode the instruction was executed in
      context = state of the CPU core trying to run the instruction,
                which may be modified as necessary.
   <= returns confirmation of emulation, if possible, or not */
pub fn emulate(priv_mode: PrivilegeMode, context: &mut IRQContext) -> EmulationResult
{
    /* get the address of the faulting instruction */
    let addr = match priv_mode
    {
        /* if it's in the hypervisor, epc will be the physical address we need */
        PrivilegeMode::Hypervisor => read_csr!(mepc) as u64,

        /* if it's in a supervisor, we need to walk the page tables.
        we can't be certain mtval contains the faulting instruction.
        it will on same systems, and not on others */
        PrivilegeMode::Supervisor =>
        {
            /* either use the translated supervisor -> hypervisor physical address
            or bail out */
            match mmu::supervisor_addr_to_phys(read_csr!(mepc) as u64)
            {
                Some(phys) => phys,
                None => return EmulationResult::CantAccess
            }
        },

        _ => return EmulationResult::CantAccess
    };

    /* check we're reading from inside the supervisor environment */
    if physmem::validate_pmp_phys_addr(addr).is_none() == true
    {
        return EmulationResult::CantAccess;
    }

    let instruction = unsafe { *(addr as *const u32) as u32 };

    /* try to enulate the rdtime instruction, which reads the 64-bit real-time clock.
    on rv32, the low 32-bits are returned. on rv64, all bits are returned */
    if (instruction & RDTIME_MASK) == RDTIME_INST
    {
        let time_now = match timer::get_pinned_timer_now_exact()
        {
            Some(t) => t as usize,
            None => return EmulationResult::CantEmulate
        };

        /* update destination register with current (low) word of the timer */
        let rd = ((instruction & !RDTIME_MASK) >> 7) & RDTIME_MASK;
        context.registers[rd as usize] = time_now;

        increment_epc(); /* go to next instuction */
        return EmulationResult::Success;
    }

    /* try to enulate the rdtime instruction, which reads the upper 32 bites of the
    64-bit real-time clock. instruction doesn't exist on non-rv32i systems */
    if cfg!(target_arch = "riscv32")
    {
        if (instruction & RDTIMEH_MASK) == RDTIMEH_INST
        {
            let time_now = match timer::get_pinned_timer_now_exact()
            {
                Some(t) => t as u64,
                None => return EmulationResult::CantEmulate
            };

            /* update destination register with current high word of the timer */
            let rd = ((instruction & !RDTIME_MASK) >> 7) & RDTIME_MASK;
            context.registers[rd as usize] = (time_now >> 32) as usize;

            increment_epc(); /* go to next instuction */
            return EmulationResult::Success;
        }
    }

    /* fall through to a confirmed illegal instruction */
    EmulationResult::IllegalInstruction
}

/* increment epc to the next 32-bit instruction */
fn increment_epc()
{
    let epc = read_csr!(mepc);
    write_csr!(mepc, epc + 4);
}