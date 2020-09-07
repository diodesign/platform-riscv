/* diosix RV32G/RV64G common hardware-specific code
 *
 * (c) Chris Williams, 2019-2020.
 *
 * See LICENSE for usage and copying.
 */

#![no_std]
#![feature(llvm_asm)]

/* basic data structures */
#[macro_use]
extern crate alloc;

/* needed to parse and generate device tree blobs */
extern crate devicetree;

/* needed for lazyily-allocated static variables, and atomic ops */
#[macro_use]
extern crate lazy_static;
extern crate spin;

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
pub mod errata;
pub mod instructions;
pub mod syscalls;
