/* diosix RV32/RV64 physical CPU core management
 *
 * (c) Chris Williams, 2019-2020.
 *
 * See LICENSE for usage and copying.
 */

#[allow(dead_code)] 

use core::fmt;
use alloc::string::String;

extern "C"
{
    fn platform_save_supervisor_state(state: &SupervisorState);
    fn platform_load_supervisor_state(state: &SupervisorState);
    fn platform_set_supervisor_return();
}

/* flags within CPUFeatures, derived from misa */
const CPUFEATURES_SUPERVISOR_MODE: usize = 1 << 18; /* supervisor mode is implemented */
const CPUFEATURES_USER_MODE: usize       = 1 << 20; /* user mode is implemented */

/* levels of privilege accepted by the hypervisor */
#[derive(Copy, Clone, Debug)]
pub enum PrivilegeMode
{
    Hypervisor, /* machine-mode hypervisor */
    Supervisor, /* supervisor */
    User        /* usermode */
}

pub type CPUcount = usize;  /* number of CPU cores in a processor or system */
pub type Reg = usize;       /* a CPU register */
pub type Entry = usize;     /* supervisor kernel entry point */

/* describe the CPU state for supervisor-level code */
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SupervisorState
{
    /* supervisor-level CSRs */
    sstatus: Reg,
    stvec: Reg,
    sip: Reg,
    sie: Reg,
    scounteren: Reg,
    sscratch: Reg,
    sepc: Reg,
    scause: Reg,
    stval: Reg,
    satp: Reg,
    pc: Entry,
    sp: Reg,
    /* standard register set (skip x0) */
    registers: [Reg; 31],
}

/* craft a blank supervisor CPU state using the given entry pointers */
pub fn supervisor_state_from(entry: Entry) -> SupervisorState
{
    SupervisorState
    {
        sstatus: 0,
        stvec: 0,
        sip: 0,
        sie: 0,
        scounteren: 0,
        sscratch: 0,
        sepc: 0,
        scause: 0,
        stval: 0,
        satp: 0,
        pc: entry,
        sp: 0,
        registers: [0; 31]
    }
}

/* save the supervisor CPU state to memory. only call from an IRQ context
   as it relies on the IRQ stacked registers. 
   => state = state area to use to store supervisor state */
pub fn save_supervisor_state(state: &SupervisorState)
{
    /* stores CSRs and x1-x31 to memory */
    unsafe { platform_save_supervisor_state(state); }
}

/* load the supervisor CPU state from memory. only call from an IRQ context
   as it relies on the IRQ stacked registers. returning to supervisor mode
   will pick up the new supervisor context.
   => state = state area to use to store supervisor state */
pub fn load_supervisor_state(state: &SupervisorState)
{
    /* stores CSRs and x1-x31 to memory */
    unsafe { platform_load_supervisor_state(state); }
}

/* run in an IRQ context. tweak necessary bits to ensure we return to supervisor mode */
pub fn prep_supervisor_return()
{
    unsafe { platform_set_supervisor_return(); }
}

/* bit masks of CPU features and extenions taken from misa */
pub type CPUFeatures = usize;

/* return the features bit mask of this CPU core */
pub fn features() -> CPUFeatures
{
    return read_csr!(misa) as CPUFeatures;
}

/* check that this CPU core has sufficient features to run code at the given privilege level
   => required = privilege level required
   <= return true if CPU can run code at the required privilege, false if not */
pub fn features_priv_check(required: PrivilegeMode) -> bool
{
    let cpu = read_csr!(misa);

    /* all RISC-V cores provide machine (hypervisor) mode. Diosix requires supervisor mode for user mode */
    match (required, cpu & CPUFEATURES_SUPERVISOR_MODE != 0, cpu & CPUFEATURES_USER_MODE != 0)
    {
        (PrivilegeMode::Hypervisor,    _,    _) => true,
        (PrivilegeMode::Supervisor, true,    _) => true,
        (      PrivilegeMode::User, true, true) => true,
        _ => false
    }
}

/* return the privilege level of the code running before we entered the machine level */
pub fn previous_privilege() -> PrivilegeMode
{
    /* previous priv level is in bts 11-12 of mstatus */
    match (read_csr!(mstatus) >> 11) & 0b11
    {
        0 => PrivilegeMode::User,
        1 => PrivilegeMode::Supervisor,
        _ => PrivilegeMode::Hypervisor
    }
}

/* define a RISC-V CPU extension in terms of its misa bit position and initial character */
struct CPUExtension
{
    bit: usize,
    initial: char
}

/* list of CPU extensions in the conventional order with their corresponding misa bit positions */
const EXTENSIONS: &'static[CPUExtension] =
    &[
        CPUExtension { bit:  8, initial: 'I' },
        CPUExtension { bit:  4, initial: 'E' },
        CPUExtension { bit: 12, initial: 'M' },
        CPUExtension { bit:  0, initial: 'A' },
        CPUExtension { bit:  5, initial: 'F' },
        CPUExtension { bit:  3, initial: 'D' },
        CPUExtension { bit:  6, initial: 'G' },
        CPUExtension { bit: 16, initial: 'Q' },
        CPUExtension { bit: 11, initial: 'L' },
        CPUExtension { bit:  2, initial: 'C' },
        CPUExtension { bit:  1, initial: 'B' },
        CPUExtension { bit:  9, initial: 'J' },
        CPUExtension { bit: 19, initial: 'T' },
        CPUExtension { bit: 15, initial: 'P' },
        CPUExtension { bit: 21, initial: 'V' },
        CPUExtension { bit: 13, initial: 'N' },
        CPUExtension { bit:  7, initial: 'H' },
        CPUExtension { bit: 25, initial: 'Z' }
    ];

/* describe a CPU core in terms of its ISA width, extensions, and architecture code name */
pub struct CPUDescription
{
    misa: usize,
    marchid: usize
}

impl CPUDescription
{
    /* create a description of this CPU core */
    pub fn new() -> CPUDescription
    {
        CPUDescription
        {
            misa: read_csr!(misa),
            marchid: read_csr!(marchid)
        }
    }
}

/* produce a human-readable version of the description */
impl fmt::Debug for CPUDescription
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        /* ISA width is stored in upper 2 bits of misa */
        let width_shift = if cfg!(target_arch = "riscv32")
        {
            32 - 2
        }
        else /* assumes RV128 is unsupported */
        {
            64 - 2
        };

        /* extract ISA width from misa to check this CPU.
        bear in mind, if this is a 32-bit hypervisor, it won't
        boot on a 64-bit system and vice versa */
        let isa = match self.misa >> width_shift
        {
            1 => 32,
            2 => 64,
            _ => return Err(fmt::Error)
        };

        let mut extensions = String::new();
        for ext in EXTENSIONS
        {
            if self.misa & (1 << ext.bit) != 0
            {
                extensions.push(ext.initial);
            }
        }

        /* taken from https://github.com/riscv/riscv-isa-manual/blob/master/marchid.md */
        let architecture = match self.marchid
        {
            0 =>  "Qemu/Unknown",
            1 =>  "Rocket",
            2 =>  "BOOM",
            3 =>  "Ariane",
            4 =>  "RI5CY",
            5 =>  "Spike",
            6 =>  "E-Class",
            7 =>  "ORCA",
            8 =>  "ORCA",
            9 =>  "YARVI",
            10 => "RVBS",
            11 => "SweRV EH1",
            12 => "MSCC",
            13 => "BlackParrot",
            14 => "BaseJump Manycore",
            15 => "C-Class",
            _ => "Unknown"
        };

        /* put it all together */
        write!(f, "RV{}{} ({})", isa, extensions, architecture)
    }
}
