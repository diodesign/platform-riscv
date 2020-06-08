/* RISC-V CSR access
 *
 * (c) Chris Williams, 2019.
 *
 * See LICENSE for usage and copying.
 */

/* read_csr(csr name) returns contents of CSR */
macro_rules! read_csr
{
    ($csr:expr) =>
    {
        unsafe
        {
            let value: usize;
            llvm_asm!(concat!("csrrs $0, ", stringify!($csr), ", x0") : "=r"(value) ::: "volatile");
            value
        }
    };
}

/* write_csr(csr name, value to write) updates csr with value */
macro_rules! write_csr
{
    ($csr:expr, $value:expr) =>
    {
        unsafe
        {
            llvm_asm!(concat!("csrrw x0, ", stringify!($csr), ", $0") :: "r"($value) :: "volatile");
        }
    };
}

/* clear_csr(csr name, mask of bits to clear) updates csr by clearing bits selected by mask */
macro_rules! clear_csr
{
    ($csr:expr, $value:expr) =>
    {
        unsafe
        {
            llvm_asm!(concat!("csrrc x0, ", stringify!($csr), ", $0") :: "r"($value) :: "volatile");
        }
    };
}
