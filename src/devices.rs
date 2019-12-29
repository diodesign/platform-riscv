/* base hardware peripherals on RV32/64 systems
 *
 * (c) Chris Williams, 2019.
 *
 * See LICENSE for usage and copying.
 */
 
/* we need this for parsing devicetrees */
extern crate devicetree;

use super::serial;
use super::physmem;
use super::timer;

/* set of basic devices for the hypervisor to use. at first, this was an elaborate
hashmap of objects describing components and peripherals but it seemed overkill. 
all we really want to do is provide the system primitives to the hypervisor:
CPU resources, RAM resources, and an outlet for debugging messages.

this structure provides access to all that. */
// #[derive(Copy)]
pub struct Devices
{
    parsed: devicetree::DeviceTree,     /* parsed device tree */

    /* frequently used stuff, cached here instead of searching the tree every time */
    nr_cpu_cores: usize,                /* number of CPU cores */
    system_ram: physmem::RAMArea,       /* describe the main system RAM area */
    debug_console: serial::SerialPort,  /* place to send debug logging */
    scheduler_timer: timer::Timer       /* periodic timer for the scheduler */ 
}

impl core::fmt::Debug for Devices
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result
    {
        write!(f, " * Parsed device tree: {:?}\n", self.parsed)?;
        
        write!(f, " * Debug console (serial port) at 0x{:x}\n", self.debug_console.get_mmio_base())?;
        write!(f, " * {} MiB of physical RAM available at 0x{:x}\n", self.system_ram.size / 1024 / 1024, self.system_ram.base)?;

        let (base, freq) = (self.scheduler_timer.get_mmio_base(), self.scheduler_timer.get_frequency());
        write!(f, " * {} Hz fixed timer(s) using CLINT at 0x{:x}\n", freq, base)?;

        write!(f, " * {} physical CPU cores", self.nr_cpu_cores)?;
        Ok(())
    }
}

impl Devices
{
    /* create a new Devices structure using the given device tree blob: the device tree
    binary is parsed to populate the structure with details of the system's base hardware
        => dtb = ptr to device tree blob to parse
        <= Some(Device) if successful, or None for failure
    */
    pub fn new(dtb: &devicetree::DeviceTreeBlob) -> Option<Devices>
    {
        let parsed = match dtb.to_parsed()
        {
            Some(p) => p,
            None => return None
        };

        /* hardwire the device structure for now with Qemu defaults */
        let d = Devices
        {
            parsed: parsed,
            nr_cpu_cores: 4,
            system_ram: physmem::RAMArea { base: 0x80000000, size: 0x20000000 },
            debug_console: serial::SerialPort::new(0x10000000),
            scheduler_timer: timer::Timer::new(10000000, 0x2000000)
        };

        return Some(d);
    }

    /* write msg string out to the debug serial port */
    pub fn write_debug_string(&self, msg: &str)
    {
        self.debug_console.write(msg);
    }

    /* return iterator describing RAM blocks available for general use */
    pub fn get_phys_ram_areas(&self) -> physmem::RAMAreaIter
    {
        physmem::validate_ram(self.nr_cpu_cores, self.system_ram)
    }

    /* enable periodic timer for PMT on this CPU core */
    pub fn scheduler_timer_start(&self) { self.scheduler_timer.start(); }

    /* interrupt this CPU core in usecs microseconds using periodic timer */
    pub fn scheduler_timer_next(&self, usecs: u64)
    {
        self.scheduler_timer.next(usecs);
    }
}
