/* Handle SBI syscalls from supervisors
 *
 * Derived from the RISC-V SBI specification: 
 * https://github.com/riscv/riscv-sbi-doc/blob/master/riscv-sbi.adoc
 * 
 * (c) Chris Williams, 2020.
 *
 * See LICENSE for usage and copying.
 */

#![allow(dead_code)]

use super::irq;

/* this implementation follows version 0.2 of the RISC-V SBI */
const SBI_SPEC_VERSION: usize = 2;

/* this is implementation ID 4, pending acceptance of:
   https://github.com/riscv/riscv-sbi-doc/pull/57 */
const SBI_IMPL_ID: usize = 4;

/* implementation version 1 */
const SBI_IMPL_VERSION: usize = 1; 

/* SBI error codes */
const SBI_SUCCESS:                      usize = 0;
const SBI_ERR_FAILED:                   usize = (-1 as i32) as usize;
const SBI_ERR_NOT_SUPPORTED:            usize = (-2 as i32) as usize;
const SBI_ERR_INVALID_PARAM:            usize = (-3 as i32) as usize;
const SBI_ERR_DENIED:                   usize = (-4 as i32) as usize;
const SBI_ERR_INVALID_ADDRESS:          usize = (-5 as i32) as usize;
const SBI_ERR_ALREADY_AVAILABLE:        usize = (-6 as i32) as usize;

/* base functionality */
const SBI_EXT_BASE: usize = 0x10;
const SBI_EXT_BASE_GET_SPEC_VERSION:    usize = 0;
const SBI_EXT_BASE_GET_IMPL_ID:         usize = 1;
const SBI_EXT_BASE_GET_IMPL_VERSION:    usize = 2;
const SBI_EXT_BASE_PROBE_EXTENSION:     usize = 3;
const SBI_EXT_BASE_GET_MVENDORID:       usize = 4;
const SBI_EXT_BASE_GET_MARCHID:         usize = 5;
const SBI_EXT_BASE_GET_MIMPLD:          usize = 6;

static SBI_EXTS: &'static [usize] = &[
    SBI_EXT_BASE
];

/* possible actions the hypervisor could take from a syscall */
#[derive(Debug)]
pub enum Action
{
    Terminate,
    Unknown(usize, usize)
}

/* parse a syscall from a supervisor from the given context,
returning an action for the hypervisor to take, if any */
pub fn handler(context: &mut irq::IRQContext) -> Option<Action>
{
    let extension = context.registers[irq::REG_A7];
    let function = context.registers[irq::REG_A6];

    match (extension, function)
    {
        /* base SBI calls */
        (SBI_EXT_BASE, SBI_EXT_BASE_GET_SPEC_VERSION) => success(context, SBI_SPEC_VERSION),
        (SBI_EXT_BASE, SBI_EXT_BASE_GET_IMPL_ID) => success(context, SBI_IMPL_ID),
        (SBI_EXT_BASE, SBI_EXT_BASE_GET_IMPL_VERSION) => success(context, SBI_IMPL_VERSION),
        (SBI_EXT_BASE, SBI_EXT_BASE_PROBE_EXTENSION) =>
        {
            let mut matched = false;

            /* run through the supported extensions and return success if there's a match */
            for extension in SBI_EXTS
            {
                if context.registers[irq::REG_A0] == *extension
                {
                    success(context, 0); /* matched an extension */
                    matched = true;
                    break;
                }
            }

            if matched == false
            {
                success(context, 1); /* did not match an extension */
            }
        },
        (SBI_EXT_BASE, SBI_EXT_BASE_GET_MVENDORID) => success(context, read_csr!(mvendorid)),
        (SBI_EXT_BASE, SBI_EXT_BASE_GET_MARCHID)   => success(context, read_csr!(marchid)),
        (SBI_EXT_BASE, SBI_EXT_BASE_GET_MIMPLD)    => success(context, read_csr!(mimpid)),

        /* catch unhandled calls */
        (e, f) => 
        {
            set_error_code(context, SBI_ERR_NOT_SUPPORTED);
            return Some(Action::Unknown(e, f))
        }
    }

    /* fall through to no action to be taken by hypervisor */
    None
}

/* set the error code of the syscall */
fn set_error_code(context: &mut irq::IRQContext, error_code: usize)
{
    context.registers[irq::REG_A0] = error_code;
}

/* set return code as success and save result */
fn success(context: &mut irq::IRQContext, result: usize)
{
    set_error_code(context, SBI_SUCCESS);
    context.registers[irq::REG_A1] = result;
}