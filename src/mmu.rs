/* diosix RV32G/RV64G memory-management code
 *
 * (c) Chris Williams, 2020.
 *
 * See LICENSE for usage and copying.
 */

const PAGE_SIZE: usize = 4 * 1024; /* system uses 4KiB pages */

const RV32_SATP_PPN_MASK: usize = (1 << 22) - 1;
const RV64_SATP_PPN_MASK: usize = (1 << 44) - 1;

const RV32_SATP_MODE_BIT_SHIFT: usize = 31;
const RV32_SATP_MODE_BIT_MASK: usize = 1;
const RV64_SATP_MODE_BIT_SHIFT: usize = 60;
const RV64_SATP_MODE_BIT_MASK: usize = 0b1111;

const RV64_SATP_MODE_SV39: usize = 8;
const RV64_SATP_MODE_SV48: usize = 9;

use super::physmem::validate_pmp_phys_addr;

/* convert supervisor address saddr to a physicalc;ea address we can use as
   a hypervisor. this derives the address from the current running
   supervisor. Returns translated physical address within the bounds
   of the supervisor environment, or None if a valid address cannot
   be derived.
   
   security note: this function deals entirely with guest-supplied
   data structures, so validate all addresses before using them.
   we check agaimst this core's PMP configutation. this won't
   change during this function as long as it is not interrupted. */
pub fn supervisor_addr_to_phys(saddr: usize) -> Option<usize>
{
   let satp = read_csr!(satp);

   if cfg!(target_arch = "riscv32")
   {
      if (satp >> RV32_SATP_MODE_BIT_SHIFT) & RV32_SATP_MODE_BIT_MASK == 0
      {
         /* no MMU active, return the 1:1 physical address mapping */
         return validate_pmp_phys_addr(saddr);
      }

      /* parse the SV32 page table structure */
      match validate_pmp_phys_addr((satp & RV32_SATP_PPN_MASK) * PAGE_SIZE)
      {
         Some(root_table) => return sv32_to_phys(root_table, saddr),
         None => return None
      }
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
      match validate_pmp_phys_addr((satp & RV64_SATP_PPN_MASK) * PAGE_SIZE)
      {
         Some(root_table) => match mode
         {
            RV64_SATP_MODE_SV39 => return sv39_to_phys(root_table, saddr),
            RV64_SATP_MODE_SV48 => return sv48_to_phys(root_table, saddr),
            _ => return None
         },
         None => return None
      }
   }

   None
}

/* page table walking code -- note: we are processing guest-supplied information.
validate physical addresses before use */

/* translate virtual address vaddr to a physical address using the page tables starting from
root_table. Returns physical address, or None if not possible. */
fn sv32_to_phys(root_table: usize, vaddr: usize) -> Option<usize>
{
   qemuprint::println!("sv32: root_table = {:x} vaddr = {:x}", root_table, vaddr);
   None
}

/* translate virtual address vaddr to a physical address using the page tables starting from
root_table. Returns physical address, or None if not possible. */
fn sv39_to_phys(root_table: usize, vaddr: usize) -> Option<usize>
{
   qemuprint::println!("sv39: root_table = {:x} vaddr = {:x}", root_table, vaddr);
   None
}

/* translate virtual address vaddr to a physical address using the page tables starting from
root_table. Returns physical address, or None if not possible. */
fn sv48_to_phys(root_table: usize, vaddr: usize) -> Option<usize>
{
   qemuprint::println!("sv48: root_table = {:x} vaddr = {:x}", root_table, vaddr);
   None
}