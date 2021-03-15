/* diosix RV64 physical CPU core management
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
    fn platform_save_supervisor_cpu_state(state: &mut SupervisorState);
    fn platform_load_supervisor_cpu_state(state: &SupervisorState);

    fn platform_save_supervisor_fp32_state(regs:  &mut FP32Registers);
    fn platform_save_supervisor_fp64_state(regs:  &mut FP64Registers);
    fn platform_load_supervisor_fp32_state(regs:  &FP32Registers);
    fn platform_load_supervisor_fp64_state(regs:  &FP64Registers);

    fn platform_set_supervisor_return();
}

/* flags within CPUFeatures, derived from misa */
const CPUFEATURES_DP_FPU: usize          = 1 << 3;  /* extension D: Double-Precision Floating-Point */
const CPUFEATURES_SP_FPU: usize          = 1 << 5;  /* extension F: Single-Precision Floating-Point */
const CPUFEATURES_SUPERVISOR_MODE: usize = 1 << 18; /* supervisor mode is implemented */
const CPUFEATURES_USER_MODE: usize       = 1 << 20; /* user mode is implemented */

/* ensure supervisor code starts in supervisor mode by setting mpp=1 in mstatus */
const MSTATUS_MPP_SUPERVISOR: Reg = 1 << 11;

/* control bits for detecting dirty state of FP registers in mstatus */
const MSTATUS_FS_SHIFT: Reg = 13; /* FS field starts at bit 13 in mstatus */
const MSTATUS_FS_MASK:  Reg = 0b11; /* FS field is 2 bits wide */
const MSTATUS_FS_DIRTY: Reg = 3; /* dirty indicates something changed FP registers */
const MSTATUS_FS_CLEAN: Reg = 2; /* clean indicates nothing changed the FP registers */
const MSTATUS_FS_OFF:   Reg = 0; /* off indicates no valid FPU present */

/* levels of privilege accepted by the hypervisor */
#[derive(Copy, Clone, Debug)]
pub enum PrivilegeMode
{
    Machine,    /* machine-mode hypervisor */
    Supervisor, /* supervisor */
    User        /* usermode */
}

pub type CPUcount = usize;  /* number of CPU cores in a processor or system */
pub type Reg = usize;       /* a CPU register */
pub type Entry = usize;     /* supervisor kernel entry point */

/* describe the CPU state for supervisor-level code */
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct SupervisorState
{
    /* supervisor-level CSRs */
    sstatus: Reg,
    sedeleg: Reg, // needs the N extension
    sideleg: Reg, // needs the N extension
    stvec: Reg,
    sip: Reg,
    sie: Reg,
    scounteren: Reg,
    sscratch: Reg,
    sepc: Reg,
    scause: Reg,
    stval: Reg,
    satp: Reg,
    mepc: Entry,
    mstatus: Entry,

    /* standard register set (skip x0) */
    registers: [Reg; 31],
}

/* supported floating-point registers, if present, are 32 or 64 bits wide.
   the 128-bit FFI ABI isn't stable yet so we'll ignore it and treat it as 64-bit for now */
type FP32Registers = [f32; 32];
type FP64Registers = [f64; 32];

/* possible supervisor-level fp registers */
enum SupervisorFPRegisters
{
    None,
    SinglePrecision(FP32Registers),
    DoublePrecision(FP64Registers)
}

/* describe FP register state for supervisor-level code */
pub struct SupervisorFPState
{
    fcsr: usize,
    registers: SupervisorFPRegisters
}

/* craft a blank supervisor CPU state and initialize it with the given entry paramters
   this state will be used to start a supervisor kernel or service.
   => cpu_nr = the virtual CPU hart ID for this supervisor CPU core
      cpu_total = total number of virtual CPU cores in this capsule
      entry = address where execution will start for this supervisor
      dtb = physical address of the device tree blob describing
            the supervisor's virtual hardware.
            it's safe to assume the RAM immediately below this is usable as
            the DTB is copied into the top-most bytes of available RAM */
pub fn init_supervisor_cpu_state(cpu_nr: CPUcount, cpu_total: CPUcount, entry: Entry, dtb: PhysMemBase) -> SupervisorState
{
    let mut state = SupervisorState
    {
        sstatus: 0,
        sedeleg: 0,
        sideleg: 0,
        stvec: 0,
        sip: 0,
        sie: 0,
        scounteren: 0,
        sscratch: 0,
        sepc: 0,
        scause: 0,
        stval: 0,
        satp: 0,
        mepc: entry,
        mstatus: MSTATUS_MPP_SUPERVISOR,
        registers: [0; 31]
    };

    /* supervisor CPU entry conditions (as per SBI and Diosix specification)
       x10 aka a0 = CPU hart ID (SBI)
       x11 aka a1 = environment's device tree blob (SBI)
       x12 aka a2 = total number of CPU harts available (Diosix)
       don't forget to skip over x0 (zero) which isn't included in the state */
    state.registers[10 - 1] = cpu_nr;
    state.registers[11 - 1] = dtb;
    state.registers[12 - 1] = cpu_total;
    state
}

/* initalize the floating-point register state for supervisor code based on the underlying physical CPU's capabilities */
pub fn init_supervisor_fp_state() -> SupervisorFPState
{
    let features = features();

    /* decode thus pCPU's misa bits F and D into the supported bit width, or 0 for no FP.
        --- = no FP hardware support
        --F = single-precision FP support
        -DF = double-precision FP support */
    let width = match (features & CPUFEATURES_DP_FPU, features & CPUFEATURES_SP_FPU)
    {
        (                 0, CPUFEATURES_SP_FPU) => 32,
        (CPUFEATURES_DP_FPU, CPUFEATURES_SP_FPU) => 64,
        _ => 0
    };

    SupervisorFPState
    {
        fcsr: 0,
        registers: match width
        {
            32 => SupervisorFPRegisters::SinglePrecision([0.0; 32]),
            64 => SupervisorFPRegisters::DoublePrecision([0.0; 32]),
            _ => SupervisorFPRegisters::None,
        }
    }
}

/* save the supervisor CPU state to memory. only call from an IRQ context
   as it relies on the IRQ stacked registers. 
   => state = state area to use to store supervisor state */
pub fn save_supervisor_cpu_state(state: &mut SupervisorState)
{
    /* stores base CSRs and x1-x31 registers to memory */
    unsafe { platform_save_supervisor_cpu_state(state); }
}

/* save the supervisor floating-point CPU state to memory
   => fp_state = state area to use to store supervisor FP state */
pub fn save_supervisor_fp_state(fp_state: &mut SupervisorFPState)
{
    /* only copy fp registers to memory if the dirty flag is set in live mstatus.
       if the FPU is not present (FS = Off) then also bail out */
    if (read_csr!(mstatus) >> MSTATUS_FS_SHIFT) & MSTATUS_FS_MASK != MSTATUS_FS_DIRTY
    {
        return;
    }

    /* store FP f0-f31 registers to memory */
    unsafe
    {
        match fp_state.registers
        {
            SupervisorFPRegisters::None => return,
            SupervisorFPRegisters::SinglePrecision(mut sp) => platform_save_supervisor_fp32_state(&mut sp),
            SupervisorFPRegisters::DoublePrecision(mut dp) => platform_save_supervisor_fp64_state(&mut dp)
        }
    }

    /* we wouldn't be here if there was no FPU, so safely read its CSR */
    fp_state.fcsr = read_csr!(fcsr);
}

/* load the supervisor CPU and FP state from memory. only call from an IRQ context
   as it relies on the IRQ stacked registers. returning to supervisor mode
   will pick up the new supervisor context.
   => state = supervisor CPU state to load from memory to registers
      fp_state = supervisor FP state to load from memory to registers*/
pub fn load_supervisor_cpu_fp_state(state: &SupervisorState, fp_state: &SupervisorFPState)
{
    /* loads base CSRs and x1-x31 into registers from memory */
    unsafe { platform_load_supervisor_cpu_state(state); }

    /* only load floating-point registers from memory if FPU is present */
    if (read_csr!(mstatus) >> MSTATUS_FS_SHIFT) & MSTATUS_FS_MASK != MSTATUS_FS_OFF
    {
        load_supervisor_fp_state(fp_state);

        /* set fs field to clean in live mstatus register. if the FP registers remain
           untouched during this timeslice then we won't waste time copying registers
           to memory */
        let mstatus = read_csr!(mstatus) & !(MSTATUS_FS_MASK << MSTATUS_FS_SHIFT);
        write_csr!(mstatus, mstatus | (MSTATUS_FS_CLEAN << MSTATUS_FS_SHIFT));
    }
}

/* load the supervisor floating-point state from memory
   => fp_state = supervisor FP state to load from memory to registers */
fn load_supervisor_fp_state(fp_state: &SupervisorFPState)
{
    /* loads FP f0-f31 registers from memory */
    unsafe
    {
        match fp_state.registers
        {
            SupervisorFPRegisters::None => return,
            SupervisorFPRegisters::SinglePrecision(sp) => platform_load_supervisor_fp32_state(&sp),
            SupervisorFPRegisters::DoublePrecision(dp) => platform_load_supervisor_fp64_state(&dp)
        }
    }

    /* we wouldn't be here if there was no FPU, so safely update its CSR */
    write_csr!(fcsr, fp_state.fcsr);
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
        (PrivilegeMode::Machine,     _,      _) => true,
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
        _ => PrivilegeMode::Machine
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

    let isa_width = if cfg!(target_arch = "riscv64")
    {
        64
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
            0 =>  "Qemu/SiFive/Unknown",
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
            18 => "SERV",
            19 => "NEORV32",
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
