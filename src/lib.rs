/* diosix RV32G/RV64G common hardware-specific code
 *
 * (c) Chris Williams, 2019.
 *
 * See LICENSE for usage and copying.
 */

#![no_std]
#![feature(asm)]

extern crate devicetree;
extern crate alloc;

/* expose architecture common code to platform-specific code */
#[macro_use]
pub mod serial;
#[macro_use]
pub mod csr;
pub mod physmem;
pub mod irq;
pub mod cpu;
pub mod timer;
pub mod test;
pub mod devices;
