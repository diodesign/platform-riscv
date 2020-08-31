/* diosix RV32G/RV64G common exception/interrupt hardware-specific code
 *
 * (c) Chris Williams, 2019-2020.
 *
 * See LICENSE for usage and copying.
 */

use super::cpu;

/* describe the type of interruption */
#[derive(Copy, Clone)]
pub enum IRQType
{
    Exception, /* software-generated interrupt */
    Interrupt, /* hardware-generated interrupt */
}

/* guide the hypervisor on whether to kill the running environment
as a result of this interrupt or exception. the hypervisor can
overrule it as required */
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum IRQSeverity
{
    Fatal, /* this IRQ should terminate the running environment */
    NonFatal /* this IRQ should not terminate the running environment */
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum IRQCause
{
    /* software interrupt generated from user, supervisor or hypervisor mode */
    UserSWI,
    SupervisorSWI,
    HypervisorSWI,
    /* hardware timer generated for user, supervisor or hypervisor mode */
    UserTimer,
    SupervisorTimer,
    HypervisorTimer,
    /* external hw interrupt generated for user, supervisor or hypervisor mode */
    UserInterrupt,
    SupervisorInterrupt,
    HypervisorInterrupt,

    /* common CPU faults */
    InstructionAlignment,
    InstructionAccess,
    IllegalInstruction,
    InstructionPageFault,
    LoadAlignment,
    LoadAccess,
    LoadPageFault,
    StoreAlignment,
    StoreAccess,
    StorePageFault,
    Breakpoint,

    /* other ways to call down from user to supervisor, etc */
    UserEnvironmentCall,
    SupervisorEnvironmentCall,
    HypervisorEnvironmentCall,

    Unknown, /* unknown, undefined, or reserved type */
}

/* describe IRQ in high-level, portable terms */
pub struct IRQ
{
    pub severity: IRQSeverity, /* hint whether this should be fatal or not */
    pub privilege_mode: crate::cpu::PrivilegeMode, /* privilege level of the interrupted code */
    pub irq_type: IRQType, /* type of the IRQ - sw or hw generated */
    pub cause: IRQCause, /* cause of this interruption */
    pub pc: usize,   /* where in memory this IRQ occurred */
    pub sp: usize,   /* stack pointer for interrupted supervisor */
}

/* Hardware-specific data from low-level IRQ handler */
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct IRQContext
{
    /* all 32 registers stacked. the contents of this array will be
    loaded into the registers on exit from the IRQ, so if you want
    to modify any register content, do it here */
    pub registers: [usize; 32],
}

/* dispatch
   Handle incoming IRQs: software exceptions and hardware interrupts
   for the high-level hypervisor.
   => context = context from the low-level code that picked up the IRQ
   <= return high-level description of the IRQ for the portable hypervisor,
      or None for no further action needs to be taken
*/
pub fn dispatch(context: IRQContext) -> Option<IRQ>
{
    /* top most bit of mcause sets what caused the IRQ: hardware or software interrupt
    thus, we need to know the width of the mcause CSR to access that top bit */
    let cause_shift = cpu::get_isa_width() - 1;
    let mcause = read_csr!(mcause);

    /* convert RISC-V cause codes into generic codes for the hypervisor.
    the top bit of the cause code is set for interrupts and clear for execeptions */
    let cause_type = match mcause >> cause_shift
    {
        0 => IRQType::Exception,
        _ => IRQType::Interrupt,
    };
    let cause_mask = (1 << cause_shift) - 1;
    let (severity, cause) = match (cause_type, mcause & cause_mask)
    {
        /* exceptions - some are labeled fatal */
        (IRQType::Exception, 0) => (IRQSeverity::Fatal, IRQCause::InstructionAlignment),
        (IRQType::Exception, 1) => (IRQSeverity::Fatal, IRQCause::InstructionAccess),
        (IRQType::Exception, 2) => (IRQSeverity::Fatal, IRQCause::IllegalInstruction),
        (IRQType::Exception, 3) => (IRQSeverity::Fatal, IRQCause::Breakpoint),
        (IRQType::Exception, 4) => (IRQSeverity::Fatal, IRQCause::LoadAlignment),
        (IRQType::Exception, 5) => (IRQSeverity::Fatal, IRQCause::LoadAccess),
        (IRQType::Exception, 6) => (IRQSeverity::Fatal, IRQCause::StoreAlignment),
        (IRQType::Exception, 7) => (IRQSeverity::Fatal, IRQCause::StoreAccess),
        (IRQType::Exception, 8) => (IRQSeverity::NonFatal, IRQCause::UserEnvironmentCall),
        (IRQType::Exception, 9) => (IRQSeverity::NonFatal, IRQCause::SupervisorEnvironmentCall),
        (IRQType::Exception, 11) => (IRQSeverity::NonFatal, IRQCause::HypervisorEnvironmentCall),
        (IRQType::Exception, 12) => (IRQSeverity::NonFatal, IRQCause::InstructionPageFault),
        (IRQType::Exception, 13) => (IRQSeverity::NonFatal, IRQCause::LoadPageFault),
        (IRQType::Exception, 15) => (IRQSeverity::NonFatal, IRQCause::StorePageFault),

        /* interrupts - none are fatal */
        (IRQType::Interrupt, 0) => (IRQSeverity::NonFatal, IRQCause::UserSWI),
        (IRQType::Interrupt, 1) => (IRQSeverity::NonFatal, IRQCause::SupervisorSWI),
        (IRQType::Interrupt, 3) => (IRQSeverity::NonFatal, IRQCause::HypervisorSWI),
        (IRQType::Interrupt, 4) => (IRQSeverity::NonFatal, IRQCause::UserTimer),
        (IRQType::Interrupt, 5) => (IRQSeverity::NonFatal, IRQCause::SupervisorTimer),
        (IRQType::Interrupt, 7) => (IRQSeverity::NonFatal, IRQCause::HypervisorTimer),
        (IRQType::Interrupt, 8) => (IRQSeverity::NonFatal, IRQCause::UserInterrupt),
        (IRQType::Interrupt, 9) => (IRQSeverity::NonFatal, IRQCause::SupervisorInterrupt),
        (IRQType::Interrupt, 11) => (IRQSeverity::NonFatal, IRQCause::HypervisorInterrupt),
        (_, _) => (IRQSeverity::NonFatal, IRQCause::Unknown),
    };

    /* return structure describing this exception to
    the high-level hypervisor for it to deal with */
    Some
    (
        IRQ
        {
            severity: severity,
            irq_type: cause_type,
            cause: cause,
            privilege_mode: crate::cpu::previous_privilege(),
            pc: read_csr!(mepc),
            sp: context.registers[2], /* x2 = sp */
        }
    )
}

/* clear an interrupt condition so we can return without the IRQ firing immediately. */
pub fn acknowledge(irq: IRQ)
{
    /* clear the appropriate pending bit in mip */
    let bit = match irq.cause
    {
        IRQCause::UserSWI               => 0,
        IRQCause::SupervisorSWI         => 1,
        IRQCause::UserTimer             => 4,
        IRQCause::SupervisorTimer       => 5,
        IRQCause::UserInterrupt         => 8,
        IRQCause::SupervisorInterrupt   => 9,
        _ => return
    };

    /* clear the pending interrupt */
    clear_csr!(mip, 1 << bit);
}
