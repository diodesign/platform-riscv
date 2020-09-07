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
    fn platform_timer_machine_enable();
    fn platform_timer_target(target: u64, clint_base: physmem::PhysMemBase);
    fn platform_timer_get_target(clint_base: physmem::PhysMemBase) -> u64;
    fn platform_timer_now(clint_base: physmem::PhysMemBase) -> u64;
    fn platform_timer_supervisor_enable();
    fn platform_timer_supervisor_trigger();
    fn platform_timer_supervisor_clear();
}

lazy_static!
{
    /* acquire PINNED_TIMER lock before accessing the timer */
    static ref PINNED_TIMER: Mutex<Option<Timer>> = Mutex::new(None);
}

/* divide timer frequency down into ticks per millisecond (1 thousandth of a second) */
const THOUSAND: u64 = 1 * 1000;
/* divide timer frequency down into ticks per microsecond (1 millionth of a second) */
const MILLION: u64 = 1 * THOUSAND * THOUSAND;
/* divide timer frequency down into ticks per nanosecond (1 billionth of a second) */
const BILLION: u64 = 1 * THOUSAND * MILLION;

/* a timer value is either in microseconds or an exact timer value */
#[derive(Debug, Clone, Copy)]
pub enum TimerValue
{
    Nanoseconds(u64),
    Microseconds(u64),
    Milliseconds(u64),
    Seconds(u64),
    Exact(u64)
}

impl TimerValue
{
    /* convert whatever the per-second value is to an
    exact timer value given the timer's freq in Hz */
    pub fn to_exact(self, freq: u64) -> u64
    {
        match self
        {
            TimerValue::Nanoseconds(t)  => (freq / BILLION) * t,
            TimerValue::Microseconds(t) => (freq / MILLION) * t,
            TimerValue::Milliseconds(t) => (freq / THOUSAND) * t,
            TimerValue::Seconds(t)      => freq * t,
            TimerValue::Exact(t)        => t
        }
    }

    pub fn to_nanoseconds(self, freq: u64) -> TimerValue
    {
        TimerValue::Nanoseconds(match self
        {
            TimerValue::Nanoseconds(t)  => t,
            TimerValue::Microseconds(t) => t * THOUSAND,
            TimerValue::Milliseconds(t) => t * MILLION,
            TimerValue::Seconds(t)      => t * BILLION,
            TimerValue::Exact(t)        => (t / freq) * BILLION
        })
    }

    pub fn to_microseconds(self, freq: u64) -> TimerValue
    {
        TimerValue::Microseconds(match self
        {
            TimerValue::Nanoseconds(t)  => t / THOUSAND,
            TimerValue::Microseconds(t) => t,
            TimerValue::Milliseconds(t) => t * THOUSAND,
            TimerValue::Seconds(t)      => t * MILLION,
            TimerValue::Exact(t)        => (t / freq) * MILLION
        })
    }

    pub fn to_milliseconds(self, freq: u64) -> TimerValue
    {
        TimerValue::Milliseconds(match self
        {
            TimerValue::Nanoseconds(t)  => t / MILLION,
            TimerValue::Microseconds(t) => t / THOUSAND,
            TimerValue::Milliseconds(t) => t,
            TimerValue::Seconds(t)      => t * THOUSAND,
            TimerValue::Exact(t)        => (t / freq) * THOUSAND
        })
    }

    pub fn to_seconds(self, freq: u64) -> TimerValue
    {
        TimerValue::Seconds(match self
        {
            TimerValue::Nanoseconds(t)  => t / BILLION,
            TimerValue::Microseconds(t) => t / MILLION,
            TimerValue::Milliseconds(t) => t / THOUSAND,
            TimerValue::Seconds(t)      => t,
            TimerValue::Exact(t)        => t / freq
        })
    }
}

/* describe a per-CPU core timer */
#[derive(Clone, Copy, Debug)]
pub struct Timer
{
    clint_base: physmem::PhysMemBase, /* base MMIO address of system's CLINT IO controller */
    frequency: u64 /* rate at which timer is incremented */
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
        self.next_in(TimerValue::Exact(0));
        /* and throw the switch... */
        unsafe { platform_timer_machine_enable(); }
    }

    /* return the current timer value. this is a clock-on-the-wall
    value in that it doesn't reset, always incremements at a fixed rate,
    though will rollover to 0 */
    pub fn get_now(&self) -> TimerValue
    {
        TimerValue::Exact(unsafe { platform_timer_now(self.clint_base) })
    }

    /* trigger an IRQ after this number of ticks or sub-seconds
       => duration = number of ticks or sub-seconds from now to interrupt */
    pub fn next_in(&self, duration: TimerValue)
    {
        let target = duration.to_exact(self.frequency) + unsafe { platform_timer_now(self.clint_base) };
        unsafe { platform_timer_target(target, self.clint_base); }
    }

    /* define the timer value after which an IRQ is triggered for this CPU core.
    => target = fire the IRQ when the timer value passes this target value */
    pub fn next_at(&self, target: TimerValue)
    {
        unsafe { platform_timer_target(target.to_exact(self.frequency), self.clint_base); }
    }

    /* get the target value that will cause the timer IRQ to fire next */
    pub fn get_next_at(&self) -> TimerValue
    {
        TimerValue::Exact(unsafe { platform_timer_get_target(self.clint_base) })
    }
}

/* return the current value of the pinned timer, or None for no pinned timer */
pub fn get_pinned_timer_now() -> Option<TimerValue>
{
    let pinned = PINNED_TIMER.lock();
    match *pinned
    {
        Some(timer) => Some(timer.get_now()),
        None => None
    }
}

/* return the frequency of the pinned timer, or None for no pinned timer */
pub fn get_pinned_timer_freq() -> Option<u64>
{
    let pinned = PINNED_TIMER.lock();
    match *pinned
    {
        Some(timer) => Some(timer.get_frequency()),
        None => None
    }
}

/* enable the supervisor's timer interrupt, trigger it, and clear a pending interrupt */
pub fn enable_supervisor_irq()  { unsafe { platform_timer_supervisor_enable();  } }
pub fn trigger_supervisor_irq() { unsafe { platform_timer_supervisor_trigger(); } }
pub fn clear_supervisor_irq()   { unsafe { platform_timer_supervisor_clear();   } }