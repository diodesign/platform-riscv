/* diosix RV32G/RV64G memory-management code
 *
 * (c) Chris Williams, 2020.
 *
 * See LICENSE for usage and copying.
 */

use super::physmem::validate_pmp_phys_addr;

/* convert supervisor address saddr to a physical address we can use as
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
      if satp >> 31 == 0
      {
         /* no MMU active, return the 1:1 physical address mapping */
         return validate_pmp_phys_addr(saddr);
      }
   }
   else if cfg!(target_arch = "riscv64")
   {
      if (satp >> 60) & 0b1111 == 0
      {
         /* no MMU active, return the 1:1 physical address mapping */
         return validate_pmp_phys_addr(saddr);
      }
   }

   None
}