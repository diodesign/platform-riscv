/* known errata on RV64 systems
 *
 * (c) Chris Williams, 2020.
 *
 * See LICENSE for usage and copying.
 */

use alloc::string::String;

/* each bit represents a bug we're aware of that needs mitigating in
   software. erratum that doesn't need fixing up in the hypervisor
   shouldn't be listed */

/* system: SiFive HiFive Unleashed A00
   SOC: FU540-C000
*/
// SIFIVE_FU540_C000_ROCK_1 -- ITIM de-allocation corrupts I-cache contents -- N/A
// SIFIVE_FU540_C000_ROCK_2 -- High 24 address bits are ignored (!)
// SIFIVE_FU540_C000_ROCK_4 -- DPC CSR is not sign-extended
const SIFIVE_FU540_C000_ROCK_3:     usize = 0; // E51 CPU atomic operations not ordered correctly
const SIFIVE_FU540_C000_CCACHE_1:   usize = 1; // L2 ECC failed address reporting flawed
const SIFIVE_FU540_C000_I2C_1:      usize = 2; // I2C interrupt can not be cleared

pub fn from_model(model: String) -> (u64, u64)
{
    let mut known: u64 = 0;
    let fixed: u64 = 0;

    if model.contains("hifive-unleashed-a00") == true
    {
        known = (1 << SIFIVE_FU540_C000_ROCK_3) | (1 << SIFIVE_FU540_C000_CCACHE_1) | (1 << SIFIVE_FU540_C000_I2C_1);
    }

    (known, fixed)
}
