/* diosix RV32G/RV64G hardware timer control for scheduler
 *
 * (c) Chris Williams, 2019.
 *
 * See LICENSE for usage and copying.
 */

use spin::Mutex;
use super::physmem;

extern "C"
{
    fn platform_timer_irq_enable();
    fn platform_timer_target(target: u64, clint_base: physmem::PhysMemBase);
    fn platform_timer_now(clint_base: physmem::PhysMemBase) -> u64;
}

lazy_static!
{
    /* acquire PINNED_TIMER lock before accessing the timer */
    static ref PINNED_TIMER: Mutex<Option<Timer>> = Mutex::new(None);
}

/* divide timer frequency down into ticks per microsecond (1 millionth) */
const MILLION: u64 = 1 * 1000 * 1000;

/* describe a per-CPU core timer */
#[derive(Clone, Copy, Debug)]
pub struct Timer
{
    clint_base: physmem::PhysMemBase, /* base MMIO address of system's CLINT IO controller */
    frequency: u64,    /* rate at which timer is incremented */
}

impl Timer
{
    /* create a new per-CPU core timer that increments a counter
       every timer tick. when the timer exceeds a target value, an IRQ is raised.
       => frequency = rate at which this timer counter increments
          clint_base = base MMIO address of the CLINT controlling this timer 
       <= per-CPU core timer object */
    pub fn new(frequency: u64, clint_base: physmem::PhysMemBase) -> Timer
    {
        Timer
        {
            clint_base: clint_base,
            frequency: frequency
        }
    }

    /* register this timer as the pinned timer, allowing other platform code to find it */
    pub fn pin(&self)
    {
        let mut pinned = PINNED_TIMER.lock();
        *pinned = Some(self.clone());
    }

    /* return base MMIO address of timer */
    pub fn get_mmio_base(&self) -> physmem::PhysMemBase { self.clint_base }

    /* return frequency of timer */
    pub fn get_frequency(&self) -> u64 { self.frequency }

    /* enable this CPU core's incremental timer interrupt */
    pub fn start(&self)
    {
        /* zero means trigger timer right away */
        self.next(0);
        /* and throw the switch... */
        unsafe { platform_timer_irq_enable(); }
    }

    /* return the current timer value right now in microseconds.
    this is a clock-on-the-wall value in that it doesn't reset,
    always incremements at a fixed rate, though will rollover to 0 */
    pub fn now(&self) -> u64
    {
        let value = unsafe { platform_timer_now(self.clint_base) };
        (value / self.frequency) * MILLION
    }

    /* define duration until this CPU core's timer next triggers an IRQ.
       => usecs = number of microseconds (millionths of a second) from now to interrupt */
    pub fn next(&self, usecs: u64)
    {
        let target = ((self.frequency / MILLION) * usecs) + unsafe { platform_timer_now(self.clint_base) };
        unsafe { platform_timer_target(target, self.clint_base); }
    }
}

/* return the current value of the pinned timer in microseconds,
or None for no pinned timer */
pub fn get_pinned_timer_now() -> Option<u64>
{
    let pinned = PINNED_TIMER.lock();
    match *pinned
    {
        Some(timer) => Some(timer.now()),
        None => None
    }
}