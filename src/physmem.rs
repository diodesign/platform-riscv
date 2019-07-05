/* diosix RV32G/RV64G code for managing physical memory
 *
 * (c) Chris Williams, 2019.
 *
 * See LICENSE for usage and copying.
 */

use core::intrinsics::transmute;
use devicetree;

/* we need this code from the assembly files */
extern "C"
{
    /* linker symbols */
    static __kernel_start: u8;
    static __kernel_end: u8;
    static __supervisor_shared_start: u8;
    static __supervisor_shared_end: u8;
    static __supervisor_data_start: u8;
    static __supervisor_data_end: u8;
}

/* place a memory barrier that ensures all RAM and MMIO read and write operations
complete in the eyes of other CPU cores before the barrier is encountered */
#[inline(always)]
pub fn barrier()
{
    unsafe
    {
        asm!("fence iorw, iorw" :::: "volatile");
    }
}

/* allowed physical memory access permissions */
pub enum AccessPermissions
{
    Read,
    ReadWrite,
    ReadExecute,
    NoAccess
}

/* there are a maximum number of physical memory regions */
const PHYS_PMP_MAX_REGIONS: usize = 15;
/* PMP access flags */
const PHYS_PMP_READ: usize  = 1 << 0;
const PHYS_PMP_WRITE: usize = 1 << 1;
const PHYS_PMP_EXEC: usize  = 1 << 2;
const PHYS_PMP_TOR: usize   = 1 << 3;

/* each CPU has a fix memory overhead, allocated during boot */
static PHYS_MEM_PER_CPU: usize = 1 << 18; /* 256KB. see ../asm/const.s */

/* standardize types for passing around physical RAM addresses */
pub type PhysMemBase = usize;
pub type PhysMemEnd  = usize;
pub type PhysMemSize = usize;

/* describe a physical RAM area using its start address and size */
pub struct RAMArea
{
    pub base: PhysMemBase,
    pub size: PhysMemSize
}

/* allow the higher level kernel to iterate over physical RAM, adding available blocks to its
allocator pool and skipping the kernel's footprint of code, data and boot payload */
pub struct RAMAreaIter
{
    total_area: RAMArea, /* describes the entire physical RAM block */
    kernel_area: RAMArea, /* describes RAM reserved by the kernel */
    pos: PhysMemBase /* current possition of the iterator into the total_area block */
}

impl Iterator for RAMAreaIter
{
    type Item = RAMArea;

    /* return a physical RAM area or None to end iteration */
    fn next(&mut self) -> Option<RAMArea>
    {
        /* if for some reason the iterator starts below phys RAM, bring it up to sanity */
        if self.pos < self.total_area.base
        {
            self.pos = self.total_area.base
        }

        /* catch the iterator escaping the physical RAM area, or if there's no phys RAM */
        if self.pos >= self.total_area.base + self.total_area.size as PhysMemBase
        {
            return None;
        }

        /* if we're in the kernel area then round us up to the end of the kernel area */
        if self.pos >= self.kernel_area.base && self.pos < self.kernel_area.base + self.kernel_area.size as PhysMemBase
        {
            self.pos = self.kernel_area.base + self.kernel_area.size as PhysMemBase;
        }

        /* determine whether we're outside a kernel area */
        if self.pos < self.kernel_area.base
        {
            /* we're below the kernel: round up from wherever we are to the kernel area base */
            let area = RAMArea
            {
                base: self.pos,
                size: (self.kernel_area.base - self.pos) as PhysMemSize
            };
            /* skip to the end of the kernel area */
            self.pos = self.kernel_area.base + self.kernel_area.size as PhysMemBase;
            return Some(area);
        }

        if self.pos >= self.kernel_area.base + self.kernel_area.size as PhysMemBase
        {
            /* we're clear of the kernel round up to end of ram */
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

/* obtain available physical memory details from system device tree. this assumes RISC-V systems have a single
   block of physical RAM. If this changes, then we need to add support for that. break up this block of physical RAM
   into one or more areas, skipping any physical RAM being used to store for the kernel, its data structures
   and boot payload to ensure this memory isn't reused for allocations.
=> device_tree_buf = device tree to parse
<= iterator that describes the available blocks of physical RAM, or None for failure */
pub fn available_ram(device_tree_buf: &u8) -> Option<RAMAreaIter>
{
    /* at the end of the kernel footprint is the per-cpu heaps in one long contiguous block.
    take this into account so the memory isn't reused for other allocations */
    let cpu_count = match devicetree::get_cpu_count(device_tree_buf)
    {
        Some(c) => c,
        None => return None
    };

    /* we'll assume the kernel, data, code, peer-CPU heaps, and its boot payload are in a contiguous block of physical RAM */
    let (phys_kernel_start, phys_kernel_end) = kernel_footprint(cpu_count);
    let phys_kernel_size = (phys_kernel_end - phys_kernel_start) as PhysMemSize;

    /* assumes RISC-V systems sport a single block of physical RAM for software use */
    let all_phys_ram = match devicetree::get_ram_area(device_tree_buf)
    {
        Some(a) => a,
        None => return None
    
    };

    /* return an iterator the higher level kernel can run through. this cuts the physical RAM
    block up into sections that do not contain the kernel footprint */
    return Some(RAMAreaIter
    {
        pos: all_phys_ram.base,
        total_area: all_phys_ram,
        kernel_area: RAMArea
        {
            base: phys_kernel_start, 
            size: phys_kernel_size
        }
    });
}

/* return the (start address, end address) of the shared supervisor kernel code in physical memory.
shared in that there is code common to the supervisor and kernel that can be shared. in effect,
this shared code appears in the supervisor's read-only code region but can be used by the hypervisor, too. */
pub fn builtin_supervisor_code() -> (PhysMemBase, PhysMemEnd)
{
    /* derived from the .sshared linker section */
    let supervisor_start: PhysMemBase = unsafe { transmute(&__supervisor_shared_start) };
    let supervisor_end: PhysMemEnd = unsafe { transmute(&__supervisor_shared_end) };
    return (supervisor_start, supervisor_end);
}

/* return the (start address, end address) of the builtin supervisor's private static read-write data
in physical memory */
pub fn builtin_supervisor_data() -> (PhysMemBase, PhysMemEnd)
{
    /* derived from the .sdata linker section */
    let supervisor_start: PhysMemBase = unsafe { transmute(&__supervisor_data_start) };
    let supervisor_end: PhysMemEnd = unsafe { transmute(&__supervisor_data_end) };
    return (supervisor_start, supervisor_end);
}

/* return the (start address, end address) of the whole kernel's code and data in physical memory,
including the builtin supervisor and fixed per-CPU core private memory areas
=> cpu_count = number of CPU cores
<= base and end addresses of the kernel footprint */
fn kernel_footprint(cpu_count: usize) -> (PhysMemBase, PhysMemEnd)
{
    /* derived from the .sshared linker section */
    let kernel_start: PhysMemBase = unsafe { transmute(&__kernel_start) };
    let kernel_end: PhysMemEnd = unsafe { transmute::<_, PhysMemEnd>(&__kernel_end) } + (cpu_count * PHYS_MEM_PER_CPU) as PhysMemEnd;
    return (kernel_start, kernel_end);
}

/* create a per-CPU physical memory region and apply access permissions to it. if the region already exists, overwrite it.
each region is a RISC-V physical memory protection (PMP) area. we pair up PMP addresses in TOR (top of range) mode. eg, region 0
uses pmp0cfg and pmp1cfg in pmpcfg0 for start and end, region 1 uses pmp1cfg and pmp2cfg in pmpcfg0.
   => regionid = ID number of the region to create or update, from 0 to PHYS_PMP_MAX_REGIONS (typically 8)
      base, end = start and end addresses of region
      access = access permissions for the region
   <= true for success, or false for failure */
pub fn protect(regionid: usize, base: usize, end: usize, access: AccessPermissions) -> bool
{
    if regionid > PHYS_PMP_MAX_REGIONS { return false; }

    let accessbits = PHYS_PMP_TOR | match access
    {
        AccessPermissions::Read => PHYS_PMP_READ,
        AccessPermissions::ReadWrite => PHYS_PMP_READ | PHYS_PMP_WRITE,
        AccessPermissions::ReadExecute => PHYS_PMP_READ | PHYS_PMP_EXEC,
        AccessPermissions::NoAccess => 0
    };

    /* select the appropriate pmpcfg register and bits from the region ID
       if we're on RV32 then there are 4 PMP regions (0-3, 4-7 etc) per 32-bit configuration word.
       if we're on RV64 then there are 8 PMP regions (0-7, 7-15) per 64-bit configuration word.

       pmpcfg_reg = N, as in: alter control register pmpcfgN to manipulate the given region
       shift = number of bits to shift left to access region in the pmpcfgN control register */    
    let (pmpcfg_reg, shift) = if cfg!(target_os = "riscv32")
    {
        /* divide the region ID by 4 (shift right twice) to convert regions 0-3 to pmpcfg0, 4-7 to pmpcfg1 etc
           then calculate shift position of the region ID in the resulting pmpcfgN register 
           (by multiplying by 8 or shifting left 3) */
        (regionid >> 2, (regionid - ((regionid >> 2) << 2)) << 3)
    }
    else /* assumes RV128 isn't available yet */
    {
        /* divide the region ID by 8 (shift right thrice) then multiply by 2 (shift left once) to
           select pmpcfg0 or 2. then calculate shift offset into the selected register */
        ((regionid >> 3) << 1, (regionid - ((regionid >> 3) << 3)) << 3)
    };

    /* only update the access bits for the end address, leaving the base access bits at zero.
    according to the specification, only the end address access bits are checked */
    let mask = 0xff << shift;
    let cfgbits = read_pmpcfg(pmpcfg_reg) & !mask;
    write_pmpcfg(pmpcfg_reg, cfgbits | (accessbits << shift));

    /* program in the base and end addesses. shift left 1 to multiply by two.
    there are a pair of PMP addresses per region: the base and the end address */
    write_pmpaddr(regionid << 1, base);
    write_pmpaddr((regionid << 1) + 1, end);

    return true;
}

/* read_pmpcfg (32-bit)
   Read the 32-bit value of the given PMP configuration register (pmpcfg0-3)
   => register = selects N out of pmpcfgN, where N = 0 to 3
   <= value of the CSR, or 0 for can't read. Warning: this fails silently, therefore */
#[cfg(target_arch = "riscv32")]
fn read_pmpcfg(register: usize) -> usize
{
    match register
    {
        0 => read_csr!(pmpcfg0),
        1 => read_csr!(pmpcfg1),
        2 => read_csr!(pmpcfg2),
        3 => read_csr!(pmpcfg3),
        _ => 0
    }
}

/* read_pmpcfg (64-bit)
   Read the 64-bit value of the given PMP configuration register (pmpcfg0 or 2)
   => register = selects N out of pmpcfgN, where N = 0 or 2
   <= value of the CSR, or 0 for can't read. Warning: this fails silently, therefore */
#[cfg(target_arch = "riscv64")]
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

/* write_pmpcfg (32-bit)
   Write value to the given PMP configuration register (pmpcfg0-3). Warning: silently fails
   => register = selects N out of pmpcfgN, where N = 0 to 3
      value = 32-bit value to write */
#[cfg(target_arch = "riscv32")]
fn write_pmpcfg(register: usize, value: usize)
{
    match register
    {
        0 => write_csr!(pmpcfg0, value),
        1 => write_csr!(pmpcfg1, value),
        2 => write_csr!(pmpcfg2, value),
        3 => write_csr!(pmpcfg3, value),
        _ => ()
    };
}

/* write_pmpcfg (64-bit)
   Write 64-bit value to the given PMP configuration register (pmpcfg0 or 2). Warning: silently fails
   => register = selects N out of pmpcfgN, where N = 0 or 2
      value = 32-bit value to write */
#[cfg(target_arch = "riscv64")]
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
fn write_pmpaddr(register: usize, value: usize)
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
