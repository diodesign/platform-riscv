/* diosix RV32G/RV64G common hardware-specific code
 *
 * (c) Chris Williams, 2019.
 *
 * See LICENSE for usage and copying.
 */

#![no_std]
#![feature(llvm_asm)]
#[macro_use]
extern crate alloc;

extern crate devicetree;

/* expose architecture common code to platform-specific code */
#[macro_use]
pub mod serial;
#[macro_use]
pub mod csr;
pub mod physmem;
pub mod virtmem;
pub mod irq;
pub mod cpu;
pub mod timer;
pub mod test;
pub mod devices;
