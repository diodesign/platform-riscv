/* diosix RV64G code for managing physical memory
 *
 * (c) Chris Williams, 2019-2020.
 *
 * See LICENSE for usage and copying.
 */

use core::intrinsics::transmute;
use super::cpu;

extern "C"
{
    /* hypervisor linker symbols */
    static __hypervisor_start: u8;
    static __hypervisor_end: u8;
}

/* place a memory barrier that ensures all RAM and MMIO read and write operations
complete in the eyes of other CPU cores before the barrier is encountered */
#[inline(always)]
pub fn barrier()
{
    unsafe
    {
        llvm_asm!("fence iorw, iorw" :::: "volatile");
    }
}

/* force a full TLB flush, needed after altering PMP and SATP CSRs */
#[inline(always)]
pub fn tlb_flush()
{
    unsafe
    {
        llvm_asm!("sfence.vma x0,x0" :::: "volatile");
    }
}

/* allowed physical memory access permissions for supervisor kernels */
#[derive(Debug)]
pub enum AccessPermissions
{
    Read,
    ReadWrite,
    ReadExecute,
    ReadWriteExecute,
    NoAccess
}

/* there are a maximum number of physical memory regions */
const PHYS_PMP_MAX_ENTRY: usize = 15;
/* PMP access flags */
const PHYS_PMP_READ: usize  = 1 << 0;
const PHYS_PMP_WRITE: usize = 1 << 1;
const PHYS_PMP_EXEC: usize  = 1 << 2;
const PHYS_PMP_TOR: usize   = 1 << 3;

/* each CPU has a fix memory overhead, allocated during boot, for its fixed heap,
exception stack, private variables, etc */
const PHYS_MEM_PER_CPU: usize = 1 << 20; /* see ../asm/const.s */

/* standardize types for passing around physical RAM addresses */
pub type PhysMemBase = usize;
pub type PhysMemEnd  = usize;
pub type PhysMemSize = usize;

/* snapshot the physical RAM control registers for debugging purposes */
#[derive(Debug, Copy, Clone)]
pub struct PhysRAMState
{
    pmpcfg0: usize,
    pmpcfg1: usize,
    pmpcfg2: usize,
    pmpcfg3: usize,

    pmpaddr0: usize,
    pmpaddr1: usize,
    pmpaddr2: usize,
    pmpaddr3: usize,

    satp: usize,
    sstatus: usize,
    stvec: usize
}

impl PhysRAMState
{
    pub fn new() -> PhysRAMState
    {
        PhysRAMState
        {
            pmpcfg0: read_pmpcfg(0),
            pmpcfg1: read_pmpcfg(1),
            pmpcfg2: read_pmpcfg(2),
            pmpcfg3: read_pmpcfg(3),

            pmpaddr0: read_csr!(pmpaddr0),
            pmpaddr1: read_csr!(pmpaddr1),
            pmpaddr2: read_csr!(pmpaddr2),
            pmpaddr3: read_csr!(pmpaddr3),

            satp: read_csr!(satp),
            sstatus: read_csr!(sstatus),
            stvec: read_csr!(stvec)
        }
    }
}

/* describe a physical RAM area using its start address and size */
#[derive(Copy, Clone, Debug)]
pub struct RAMArea
{
    pub base: PhysMemBase,
    pub size: PhysMemSize
}

/* allow the hardware-independent code to iterate over physical RAM, adding available blocks to its
allocator pool and skipping the hypervisor's footprint of code, data and boot payload */
pub struct RAMAreaIter
{
    total_area: RAMArea, /* describes the entire physical RAM block */
    hypervisor_area: RAMArea, /* describes RAM reserved by the hypervisor */
    pos: PhysMemBase /* current position of the iterator into the total_area block */
}

impl Iterator for RAMAreaIter
{
    type Item = RAMArea;

    /* return a physical RAM area or None to end iteration */
    fn next(&mut self) -> Option<RAMArea>
    {
        /* if for some reason the iterator starts below phys RAM, bring it up to reality */
        if self.pos < self.total_area.base
        {
            self.pos = self.total_area.base
        }

        /* catch the iterator escaping the physical RAM area, or if there's no phys RAM */
        if self.pos >= self.total_area.base + self.total_area.size as PhysMemBase
        {
            return None;
        }

        /* if we're in the hypervisor area then round us up to the end of the hypervisor area */
        if self.pos >= self.hypervisor_area.base && self.pos < self.hypervisor_area.base + self.hypervisor_area.size as PhysMemBase
        {
            self.pos = self.hypervisor_area.base + self.hypervisor_area.size as PhysMemBase;
        }

        /* determine whether we're below the hypervisor area */
        if self.pos < self.hypervisor_area.base
        {
            /* we're below the hypervisor: round up from wherever we are to the hypervisor area base */
            let area = RAMArea
            {
                base: self.pos,
                size: (self.hypervisor_area.base - self.pos) as PhysMemSize
            };
            /* skip to the end of the hypervisor area */
            self.pos = self.hypervisor_area.base + self.hypervisor_area.size as PhysMemBase;
            return Some(area);
        }

        /* or if we're above or in the hypervisor area */
        if self.pos >= self.hypervisor_area.base + self.hypervisor_area.size as PhysMemBase
        {
            /* we're clear of the hypervisor, so round up to end of ram */
            let area = RAMArea
            {
                base: self.pos,
                size: ((self.total_area.base + self.total_area.size) - self.pos) as PhysMemSize
            };
            self.pos = self.total_area.base + self.total_area.size as PhysMemBase;
            return Some(area);
        }

        /* if we fall through to here then stop the iterator */
        return None;
    }
}

/* Iterate over a block of RAM, skipping the hypervisor, its boot capsule, and any per-CPU core data structures,
   and returning just blocks of RAM that can be allocated and used by capsules and the hypervisor as needed.
   In other words, pass a RAMArea of physical memory, and this will return an iterator of allocatable memory blocks
    => cpu_count = number of physical CPU cores present in the machine
       phys_ram_blocks = Vector list of RAMAreas describing this block of physical RAM
    <= iterator that describes the available blocks of physical RAM */
pub fn validate_ram(cpu_count: usize, phys_ram_block: RAMArea) -> RAMAreaIter
{
    /* we'll assume the hypervisor, data, code, per-CPU heaps, and its boot payload are in a contiguous block of physical RAM */
    let (phys_hypervisor_start, phys_hypervisor_end) = hypervisor_footprint(cpu_count);
    let phys_hypervisor_size = (phys_hypervisor_end - phys_hypervisor_start) as PhysMemSize;

    /* return an iterator the higher level hypervisor can run through. this cuts the physical RAM
    block up into sections that do not contain the hypervisor footprint */
    RAMAreaIter
    {
        pos: phys_ram_block.base,
        total_area: phys_ram_block,
        hypervisor_area: RAMArea
        {
            base: phys_hypervisor_start, 
            size: phys_hypervisor_size
        }
    }
}

/* return the (start address, end address) of the whole hypervisor's code and data in physical memory,
   this must include the fixed per-CPU core private memory areas.
    => cpu_count = number of CPU cores
    <= base and end addresses of the hypervisor footprint */
fn hypervisor_footprint(cpu_count: usize) -> (PhysMemBase, PhysMemEnd)
{
    /* derived from the .sshared linker section */
    let hypervisor_start: PhysMemBase = unsafe { transmute(&__hypervisor_start) };
    let hypervisor_end: PhysMemEnd = unsafe { transmute::<_, PhysMemEnd>(&__hypervisor_end) } + (cpu_count * PHYS_MEM_PER_CPU) as PhysMemEnd;
    return (hypervisor_start, hypervisor_end);
}

/* Control currently running supervisor kernel's access to a region of physical memory. Either use PMP or CPU hypervisor extension,
   depending on whatever is available, to enforce this. So far, just PMP is supported.
   => base, end = start and end addresses of physical RAM region
      access = access permissions for the region for the currently running supervisor kernel
   <= true for success, or false for failure */
pub fn protect(base: usize, end: usize, access: AccessPermissions) -> bool
{
    return pmp_protect(0, base, end, access);
}

/* define a per-CPU physical memory region and apply access permissions to it. if the region already exists, overwrite it.
each region is a pair of RISC-V physical memory protection (PMP) area. we pair up PMP addresses in TOR (top of range) mode.
eg, region 0 uses pmp0cfg and pmp1cfg in pmpcfg0 for start and end, region 1 uses pmp1cfg and pmp2cfg in pmpcfg0.
   => regionid = ID number of the region to create or update, from 0 to PHYS_PMP_MAX_REGIONS (typically 8).
                 Remember: one region is a pair of PMP entries
      base, end = start and end addresses of region
      access = access permissions for the region
   <= true for success, or false for failure */
fn pmp_protect(region_id: usize, base: usize, end: usize, access: AccessPermissions) -> bool
{
    /* here are two PMP entries to one diosix region: one for base address, one for the end address */
    let pmp_entry_base_id = region_id * 2;
    let pmp_entry_end_id = pmp_entry_base_id + 1;
    if pmp_entry_end_id > PHYS_PMP_MAX_ENTRY { return false; }

    let accessbits = match access
    {
        AccessPermissions::Read => PHYS_PMP_READ,
        AccessPermissions::ReadWrite => PHYS_PMP_READ | PHYS_PMP_WRITE,
        AccessPermissions::ReadExecute => PHYS_PMP_READ | PHYS_PMP_EXEC,
        AccessPermissions::ReadWriteExecute => PHYS_PMP_READ | PHYS_PMP_WRITE | PHYS_PMP_EXEC,
        AccessPermissions::NoAccess => 0
    };

    /* update the appropriate pmpcfg register and bits from the PMP entry ID */
    /* clear the base address's settings: only the end address is used */
    write_pmp_entry(pmp_entry_base_id, 0);
    /* do the end address's settings and make it TOR (top of range) */
    write_pmp_entry(pmp_entry_end_id, accessbits | PHYS_PMP_TOR);

    /* program in the actual base and end addresses. there are a pair of PMP addresses
    per region: the base and the end address. they are also shifted down two bits
    because that's exactly what the spec says. word alignment, right? */
    write_pmp_addr(pmp_entry_base_id, base >> 2);
    write_pmp_addr(pmp_entry_end_id, end >> 2);

    /* force a reload of MMU data structures */
    tlb_flush();
    return true;
}

/* write_pmp_entry
   Update settings flags exclusively for given PMP entry (typically 0 to 15) in pmpcfg[0-3] registers
   => entry_id = PMP entry to alter (0-15)
      value = settings flags to write (only low byte is used) */
fn write_pmp_entry(entry_id: usize, value: usize)
{
    let (pmp_cfg_id, offset) = match cpu::get_isa_width()
    {
        /* for RV32 targets only */
        /* 32 =>
        {
            // four PMP entries to a 32-bit pmpcfg register
            let pmp_cfg_id = entry_id >> 2;
            let offset = entry_id - (pmp_cfg_id << 2);
            (pmp_cfg_id, offset)
        }, */

        64 =>
        {
            /* eight PMP entries to a 64-bit pmpcfg register */
            let pmp_cfg_id = entry_id >> 3;
            let offset = entry_id - (pmp_cfg_id << 3);
            (pmp_cfg_id, offset)
        },

        /* avoid panic() though in this case, we're targeting an unsupported
        architecture, so quit while we're behind */
        w => panic!("Can't write PMP entry: unsupported ISA width {}", w)
    };

    /* eight bits per PMP entry. use masking to avoid changing other entries' settings */
    let mask: usize = 0xff << (offset << 3);
    let cfgbits = read_pmpcfg(pmp_cfg_id) & !mask;
    write_pmpcfg(pmp_cfg_id, cfgbits | ((value & 0xff) << (offset << 3)));
}

/* read_pmpcfg
   Read the 64-bit value of the given PMP configuration register (pmpcfg0 or 2)
   => register = selects N out of pmpcfgN, where N = 0 or 2
   <= value of the CSR, or 0 for can't read. Warning: this fails silently, therefore */
fn read_pmpcfg(register: usize) -> usize
{
    /* we must conditionally compile this because pmpcfg1 and pmpcfg3 aren't defined for riscv64 */
    match register
    {
        0 => read_csr!(pmpcfg0),
        2 => read_csr!(pmpcfg2),
        _ => 0
    }
}

/* write_pmpcfg
   Write 64-bit value to the given PMP configuration register (pmpcfg0 or 2). Warning: silently fails
   => register = selects N out of pmpcfgN, where N = 0 or 2
      value = 32-bit value to write */
fn write_pmpcfg(register: usize, value: usize)
{
    /* we must conditionally compile this because pmpcfg1 and pmpcfg3 aren't defined for riscv64 */
    match register
    {
        0 => write_csr!(pmpcfg0, value),
        2 => write_csr!(pmpcfg2, value),
        _ => ()
    };
}

/* write value to the given PMP address register 0-15 (pmpaddr0-15). warning: silently fails */
fn write_pmp_addr(register: usize, value: usize)
{
    match register
    {
        0 => write_csr!(pmpaddr0, value),
        1 => write_csr!(pmpaddr1, value),
        2 => write_csr!(pmpaddr2, value),
        3 => write_csr!(pmpaddr3, value),
        4 => write_csr!(pmpaddr4, value),
        5 => write_csr!(pmpaddr5, value),
        6 => write_csr!(pmpaddr6, value),
        7 => write_csr!(pmpaddr7, value),
        8 => write_csr!(pmpaddr8, value),
        9 => write_csr!(pmpaddr9, value),
        10 => write_csr!(pmpaddr10, value),
        11 => write_csr!(pmpaddr11, value),
        12 => write_csr!(pmpaddr12, value),
        13 => write_csr!(pmpaddr13, value),
        14 => write_csr!(pmpaddr14, value),
        15 => write_csr!(pmpaddr15, value),
        _ => ()
    };
}
