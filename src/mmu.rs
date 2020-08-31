/* diosix RV32G/RV64G memory-management code
 *
 * (c) Chris Williams, 2020.
 *
 * See LICENSE for usage and copying.
 */

use super::physmem::validate_pmp_phys_addr;

const PAGE_SIZE:        u64 = 4 * 1024; /* system uses 4KiB pages */
const PAGE_OFFSET_MASK: u64 = PAGE_SIZE - 1;

const RV32_SATP_PPN_MASK:        u64 = (1 << 22) - 1;
const RV64_SATP_PPN_MASK:        u64 = (1 << 44) - 1;

const RV32_SATP_MODE_BIT_SHIFT:  u64 = 31;
const RV32_SATP_MODE_BIT_MASK:   u64 = 1;
const RV64_SATP_MODE_BIT_SHIFT:  u64 = 60;
const RV64_SATP_MODE_BIT_MASK:   u64 = 0b1111;

const RV64_SATP_MODE_SV39:       u64 = 8;
const RV64_SATP_MODE_SV48:       u64 = 9;

const SV39_VADDR_MASK:           u64 = (1 << 39) - 1;
const SV39_VPN_BASE_SHIFT:       u64 = 12;
const SV39_VPN_SHIFT:            u64 = 9;
const SV39_VPN_COUNT:            u64 = 3;
const SV39_VPN_MASK:             u64 = (1 << 10) - 1;
const SV39_TABLE_ENTRIES:        usize = 512;
const SV39_PTE_PPN_BASE_SHIFT:   u64 = 10;
const SV39_PTE_PPN_SHIFT:        u64 = 9;
const SV39_PTE_PPN_MASK:         u64 = (1 << 10) - 1;
const SV39_PTE_PPN_FULL_MASK:    u64 = (1 << 44) - 1;
const SV39_PHYS_PPN_BASE_SHIFT:  u64 = 12;
const SV39_PHYS_PPN_SHIFT:       u64 = 9;

const PAGE_BITS_VALID:  u8 = 1 << 0;
const PAGE_BITS_READ:   u8 = 1 << 1;
const PAGE_BITS_WRITE:  u8 = 1 << 2;
const PAGE_BITS_EXEC:   u8 = 1 << 3;
const PAGE_RWX_MASK:    u8 = PAGE_BITS_READ | PAGE_BITS_WRITE | PAGE_BITS_EXEC;

type SV39PageTable = [u64; SV39_TABLE_ENTRIES];

/* convert supervisor address saddr to a physical address we can use as
   a hypervisor. this derives the address from the current running
   supervisor. Returns translated physical address within the bounds
   of the supervisor environment, or None if a valid address cannot
   be derived.
   
   security note: this function deals entirely with guest-supplied
   data structures, so validate all addresses before using them.
   we check agaimst this core's PMP configutation. this won't
   change during this function as long as it is not interrupted. */
pub fn supervisor_addr_to_phys(saddr: u64) -> Option<u64>
{
   let satp = read_csr!(satp) as u64;

   if cfg!(target_arch = "riscv32")
   {
      if (satp >> RV32_SATP_MODE_BIT_SHIFT) & RV32_SATP_MODE_BIT_MASK == 0
      {
         /* no MMU active, return the 1:1 physical address mapping */
         return validate_pmp_phys_addr(saddr);
      }

      /* parse the SV32 page table structure */
      let root_table = (satp & RV32_SATP_PPN_MASK) * PAGE_SIZE;
      return sv32_to_phys(root_table, saddr);
   }
   else if cfg!(target_arch = "riscv64")
   {
      let mode = (satp >> RV64_SATP_MODE_BIT_SHIFT) & RV64_SATP_MODE_BIT_MASK;
      if mode == 0
      {
         /* no MMU active, return the 1:1 physical address mapping */
         return validate_pmp_phys_addr(saddr);
      }

      /* parse the correct page table structure */
      let root_table = (satp & RV64_SATP_PPN_MASK) * PAGE_SIZE;
      match mode
      {
         RV64_SATP_MODE_SV39 => return sv39_to_phys(root_table, saddr),
         RV64_SATP_MODE_SV48 => return sv48_to_phys(root_table, saddr),
         _ => return None
      }
   }

   None
}

/* page table walking code -- note: we are processing guest-supplied information.
validate physical addresses before use to ensure a guest doesn't try to use out
of bounds data as a page table. trap faults as errors in the supervisor.
PMP configuration can't change on this core while we're running so validation
checks should hold, provided this code isn't interrupted */

/* translate virtual address vaddr to a physical address using the page tables starting from
root_table_addr. Returns physical address, or None if not possible. */
fn sv32_to_phys(_root_table_addr: u64, _vaddr: u64) -> Option<u64>
{
   None
}

/* translate virtual address vaddr to a physical address using the page tables starting from
table_addr. Returns physical address if vaddr resolves to a readable/executable page,
or None if not possible. */
fn sv39_to_phys(mut table_addr: u64, vaddr: u64) -> Option<u64>
{
   let vaddr = vaddr & SV39_VADDR_MASK;
   let page_offset = vaddr & PAGE_OFFSET_MASK;

   /* count from vpn2 to vpn0 in vaddr */
   for vpn in (0..SV39_VPN_COUNT).rev()
   {
      /* validate the page table addressses */
      if validate_pmp_phys_addr(table_addr).is_none() == true ||
         validate_pmp_phys_addr(table_addr + PAGE_SIZE - 1).is_none() == true
      {
         return None;
      }

      let table: SV39PageTable = unsafe { *(table_addr as *const SV39PageTable) };

      /* decode vaddr into virtual page numbers */
      let shift = SV39_VPN_BASE_SHIFT + (vpn * SV39_VPN_SHIFT);
      let entry_index = (vaddr >> shift) & SV39_VPN_MASK;

      /* get read-write-execute access bits for this page table entry */
      let entry = table[entry_index as usize];
      let entry_rwx = entry as u8 & PAGE_RWX_MASK;

      /* bail out if we run into an invalid page */
      if entry as u8 & PAGE_BITS_VALID == PAGE_BITS_VALID
      {
         /* if RWX is zero then this is an entry to another table */
         if entry_rwx == 0
         {
            table_addr = ((entry >> SV39_PTE_PPN_BASE_SHIFT) & SV39_PTE_PPN_FULL_MASK as u64) as u64;
            table_addr = table_addr * PAGE_SIZE;
         }
         else
         {
            /* access bits are defined so this is a leaf node.
            if read or execute aren't set, then as per the spec, fail this lookup */
            if entry_rwx & PAGE_BITS_EXEC == PAGE_BITS_EXEC ||
               entry_rwx & PAGE_BITS_READ == PAGE_BITS_READ
            {
               /* build the physical address */
               let mut paddr: u64 = page_offset as u64;

               if vpn > 0
               {
                  /* we're in a super page */
                  for index in (vpn..SV39_VPN_COUNT).rev()
                  {
                     let pte_ppn_shift = SV39_PTE_PPN_BASE_SHIFT + (SV39_PTE_PPN_SHIFT * index);
                     let paddr_ppn_shift = SV39_PHYS_PPN_BASE_SHIFT + (SV39_PHYS_PPN_SHIFT * index);

                     let pte_ppn = (entry >> pte_ppn_shift) & SV39_PTE_PPN_MASK as u64;
                     paddr = paddr | (pte_ppn << paddr_ppn_shift);
                  }
                  for index in (0..vpn).rev()
                  {
                     let vpn_shift = SV39_VPN_BASE_SHIFT + (SV39_VPN_SHIFT * index);
                     let paddr_ppn_shift = SV39_PHYS_PPN_BASE_SHIFT + (SV39_PHYS_PPN_SHIFT * index);

                     let pte_ppn = (vaddr as u64 >> vpn_shift) & SV39_VPN_MASK as u64;
                     paddr = paddr | (pte_ppn << paddr_ppn_shift);
                  }

                  return Some(paddr);
               }
               else
               {
                  /* we're in a normal 4KB page */
                  let entry_phys_addr = (entry >> SV39_PTE_PPN_BASE_SHIFT) & SV39_PTE_PPN_FULL_MASK;
                  paddr = paddr | (entry_phys_addr << SV39_PHYS_PPN_BASE_SHIFT);
                  return Some(paddr);
               }
            }
            else
            {
               return None;
            }
         }
      }
      else
      {
         return None;
      }
   }

   None
}

/* translate virtual address vaddr to a physical address using the page tables starting from
root_table_addr. Returns physical address, or None if not possible. */
fn sv48_to_phys(_root_table_addr: u64, _vaddr: u64) -> Option<u64>
{
   None
}
