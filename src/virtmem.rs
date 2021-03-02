/* diosix RV64G code for managing guest virtual memory
 *
 * (c) Chris Williams, 2019.
 *
 * See LICENSE for usage and copying.
 */

/* standardize types for passing around guest virtual RAM addresses */
pub type VirtMemBase = usize;
pub type VirtMemEnd  = usize;
pub type VirtMemSize = usize;
