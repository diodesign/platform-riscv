/* diosix RV32/RV64 physical CPU core management
 *
 * (c) Chris Williams, 2019-2020.
 *
 * See LICENSE for usage and copying.
 */

#[allow(dead_code)] 

use core::fmt;
use alloc::string::String;
use super::physmem::PhysMemBase;

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

/* craft a blank supervisor CPU state and initialize it with the given entry paramters
   this state will be used to start a supervisor kernel.
   => cpu_nr = the CPU hart ID for this supervisor CPU core
      entry = address where execution will start for this supervisor
      dtb = physical address of the device tree blob describing
            the supervisor's virtual hardware */
pub fn init_supervisor_state(cpu_nr: CPUcount, entry: Entry, dtb: PhysMemBase) -> SupervisorState
{
    let mut state = SupervisorState
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
    };

    /* supervisor CPU entry conditions (as expected by the Linux kernel):
       x10 aka a0 = CPU hart ID
       x11 aka a1 = environment's device tree blob
       don't forget to skip over x0 (zero) which isn't included in the state */
    state.registers[10 - 1] = cpu_nr;
    state.registers[11 - 1] = dtb;
    state
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

/* bit masks of CPU features and extensions taken from misa */
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

/* returns the running CPU core's ISA width in bits */
pub fn get_isa_width() -> usize
{
    /* we should test misa for ISA width, though it's moot because
    if the machine was, say, RV64, it would only boot an RV64-targeted
    hypervisor due to differences in the RV32, RV64 and RV128 instruction
    formats. for now, we can check the hypervisor target at build time.

    TODO: check misa */

    let isa_width = if cfg!(target_arch = "riscv32")
    {
        32
    }
    else if cfg!(target_arch = "riscv64")
    {
        64
    }
    else if cfg!(target_arch = "riscv128")
    {
        128
    }
    else
    {
        /* avoid panic() though in this case, if we're building for an
        unexpected architecture, then something's gone quite wrong
        and it's likely we haven't made it this far into runtime anyway */
        panic!("Unexpected target architecture {}", cfg!(target_arch));
    };

    isa_width
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

/* describe the running CPU core in terms of its ISA width, extensions, and architecture code name */
pub struct CPUDescription;

impl CPUDescription
{
    /* generate a string describing the ISA in the usual RISC-V format:
    RV32 or RV64 followed by extension characters, all uppercase, no spaces,
    eg: RV32IMAFD */
    pub fn isa_to_string(&self) -> String
    {
        let misa = read_csr!(misa);
        let mut extensions = String::new();
        for extension in EXTENSIONS
        {
            if misa & (1 << extension.bit) != 0
            {
                extensions.push(extension.initial);
            }
        }

        /* combine ISA width and extension letters */
        format!("RV{}{}", get_isa_width(), extensions)
    }

    /* return a string describing the CPU core's microarchitecture */
    pub fn arch_to_string(&self) -> String
    {
        /* taken from https://github.com/riscv/riscv-isa-manual/blob/master/marchid.md
        TODO: Automate this list? */
        format!("{}", match read_csr!(marchid)
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
            16 => "SweRV EL2",
            17 => "SweRV EH2",
            _ => "Unknown"
        })
    }
}

/* produce a human-readable version of the CPU description. for RISC-V,
it's the ISA width and extensions followed by a space and then the
microarchirecture in brackets, eg: RV64IMAFDC (Qemu/Unknown) */
impl fmt::Debug for CPUDescription
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        write!(f, "{} ({})", self.isa_to_string(), self.arch_to_string())
    }
}
