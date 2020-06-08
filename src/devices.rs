/* base hardware peripherals on RV32/64 systems
 *
 * discover hardware from a firmware-supplied device tree 
 * 
 * (c) Chris Williams, 2019-2020
 *
 * See LICENSE for usage and copying.
 */
 
extern crate devicetree;
use devicetree::{DeviceTree, DeviceTreeBlob, DeviceTreeError, DeviceTreeProperty, DeviceTreeIterDepth};

use super::serial;
use super::physmem;
use super::timer;

use alloc::string::String;
use alloc::vec::Vec;

/* set of basic devices for the hypervisor to use. at first, this was an elaborate
hashmap of objects describing components and peripherals but it seemed overkill. 
all we really want to do is provide the system primitives to the hypervisor:
CPU resources, RAM resources, and an outlet for debugging messages.

this structure provides access to all that. */
#[derive(Debug)]
pub struct Devices
{
    parsed: devicetree::DeviceTree,             /* parsed device tree */

    /* frequently used stuff, cached here instead of searching the tree every time */
    nr_cpu_cores: usize,                        /* number of logical CPU cores */
    system_ram: Vec<physmem::RAMArea>,          /* list of physical RAM chunks */
    debug_console: Option<serial::SerialPort>,  /* place to send debug logging, if possible */
    scheduler_timer: Option<timer::Timer>       /* periodic timer for the scheduler */ 
}

impl Devices
{
    /* create a new Devices structure using the given device tree blob: the device tree
    binary is parsed to populate the structure with details of the system's base hardware
        => dtb = ptr to device tree blob to parse
        <= Some(Devices) if successful, or None for failure
    */
    pub fn new(dtb: &DeviceTreeBlob) -> Result<Devices, DeviceTreeError>
    {
        let parsed = dtb.to_parsed()?;

        /* fill out the minimum default devices expected by the hypervisor from parsed DTB */
        let d = Devices
        {
            nr_cpu_cores:
            {
                /* cells.address = #address-cells for /cpus, which is the number of u32 cells
                per logical CPU core ID number in each physical CPU node (see below and spec).
                typically this value is 1. */
                let cells = parsed.get_address_size_cells(&format!("/cpus"));

                let mut count = 0;
                for node in parsed.iter(&format!("/cpus/cpu"), 2)
                {
                    /* each physical core can contain N independent logical CPU cores, the ID numbers for
                    which are stored in an array in the reg property of each /cpus/cpu* entry. count up how
                    many ID entries are present in each physical core's reg array to determine total
                    number of expected logical CPU cores */
                    let cpu_ids = parsed.get_property(&node, &format!("reg"))?.as_multi_u32()?;
                    count = count + (cpu_ids.len() / cells.address);
                }
                count
            },
            
            debug_console: match setup_debug_console(&parsed)
            {
                Ok(dc) => Some(dc), /* use a suitable serial or debug port for output */
                Err(_) => None /* no serial console, no way to warn the user :-( */
            },

            system_ram:
            {
                /* the device tree describes large chunks of physical RAM that may not
                entirely be available for use. add these chunks to a list for processing later */
                let mut chunks = Vec::new();
                for path in parsed.iter(&format!("/memory@"), 1)
                {
                    if let Ok(chunk) = get_ram_chunk(&parsed, &path)
                    {
                        chunks.push(chunk);
                    }
                }
                chunks
            },

            scheduler_timer:
            {
                /* use the first CLINT found in the tree for our system timer */
                if let Some(path) = parsed.iter(&format!("/soc/clint@"), 2).next()
                {
                    match get_system_timer(&parsed, &path)
                    {
                        Ok(t) => Some(t),
                        Err(_) => None /* would be nice to flag up error */
                    }
                }
                else
                {
                    None
                }
            },

            parsed: parsed,
        };

        Ok(d)
    }

    /* write msg string out to the debug serial port */
    pub fn write_debug_string(&self, msg: &str)
    {
        if let Some(con) = self.debug_console
        {
            con.write(msg);
        }
    }

    /* return number of discovered logical CPU cores */
    pub fn get_nr_cpu_cores(&self) -> usize { self.nr_cpu_cores }

    /* return vector list of RAM blocks available for general use */
    pub fn get_phys_ram_areas(&self) -> Vec<physmem::RAMArea> { self.system_ram.clone() }

    /* enable periodic timer for PMT on this CPU core */
    pub fn scheduler_timer_start(&self)
    {
        if let Some(s) = self.scheduler_timer
        {
            s.start();
        }
    }

    /* interrupt this CPU core in usecs microseconds using periodic timer */
    pub fn scheduler_timer_next(&self, usecs: u64)
    {
        if let Some(s) = self.scheduler_timer
        {
            s.next(usecs);
        }
    }
}

/* find a suitable serial port for the debug console and create the SerialPort object for it,
or return an error code */
fn setup_debug_console(dt: &DeviceTree) -> Result<serial::SerialPort, DeviceTreeError>
{
    /* check if the firmware has chosen a specific device for debug output */
    if let Ok(node) = dt.get_property(&format!("/chosen"), &format!("stdout-path"))
    {
        if let Ok(path) = node.as_text()
        {
            return create_debug_console(&dt, &path);
        }
    }

    /* search aliased locations for up to four serial nodes for the debug console.
    feel free to make this is a little more smarter (why four, for example?) */
    for idx in 0..4
    {
        if let Ok(node) = dt.get_property(&format!("/aliases"), &format!("serial{}", idx))
        {
            if let Ok(alias_path) = node.as_text()
            {
                return create_debug_console(&dt, &alias_path);
            }
        }
    }

    Err(DeviceTreeError::NotFound)
}

/* create a SerialPort object from the given devicetree node, or return an error */
fn create_debug_console(dt: &DeviceTree, path: &String) -> Result<serial::SerialPort, DeviceTreeError>
{
    /* get the width of the serial port's hardware address from the parent node */
    let parent = devicetree::get_parent(path);
    let cells = dt.get_address_size_cells(&parent);

    /* the base address of the serial port is the first value in the reg list */
    let reg = match dt.get_property(path, &format!("reg"))
    {
        Ok(r) => r,
        Err(e) => return Err(e)
    };

    /* the base address may be either 32-bit or 64-bit in size */
    match cells.address
    {
        1 => Ok(serial::SerialPort::new(reg.as_multi_u32()?[0] as usize)),
        2 => Ok(serial::SerialPort::new(reg.as_multi_u64()?[0] as usize)),
        _ => Err(DeviceTreeError::WidthUnsupported)
    }
}

/* return a RAMArea describing the given devicetree /memory node, or error for failure */
fn get_ram_chunk(dt: &DeviceTree, path: &String) -> Result<physmem::RAMArea, DeviceTreeError>
{
    /* get the width of the memory area's base address and size from the parent node */
    let parent = devicetree::get_parent(path);
    let cells = dt.get_address_size_cells(&parent);

    /* the base address and size are stored consecutively in the reg list */
    let reg = match dt.get_property(path, &format!("reg"))
    {
        Ok(r) => r,
        Err(e) => return Err(e)
    };

    /* the address and size may be either 32-bit or 64-bit in length */
    match (cells.address, cells.size)
    {
        (1, 1) =>
        {
            Ok(physmem::RAMArea
            {
                base: reg.as_multi_u32()?[0] as physmem::PhysMemBase,
                size: reg.as_multi_u32()?[1] as physmem::PhysMemSize
            })
        },
        (2, 2) =>
        {
            Ok(physmem::RAMArea
            {
                base: reg.as_multi_u64()?[0] as physmem::PhysMemBase,
                size: reg.as_multi_u64()?[1] as physmem::PhysMemSize
            })
        },
        (_, _) => Err(DeviceTreeError::WidthUnsupported)
    }
}

/* return a new timer from the given device tree CLINT node, or error for failure */
fn get_system_timer(dt: &DeviceTree, path: &String) -> Result<timer::Timer, DeviceTreeError>
{
    /* get timebase frequency: assume it's a 32-bit value? :( */
    let tbf = match dt.get_property(&format!("/cpus"), &format!("timebase-frequency"))
    {
        Ok(f) => f.as_u32()? as u64,
        Err(e) => return Err(e)
    };

    /* get the width of the CLINT's addresses and sizes */
    let parent = devicetree::get_parent(path);
    let cells = dt.get_address_size_cells(&parent);

    /* get base address of the CLINT from its reg property */
    let reg = match dt.get_property(path, &format!("reg"))
    {
        Ok(r) => r,
        Err(e) => return Err(e)
    };

    /* the base address may be either 32-bit or 64-bit in size */
    match cells.address
    {
        1 => Ok(timer::Timer::new(tbf, reg.as_multi_u32()?[0] as usize)),
        2 => Ok(timer::Timer::new(tbf, reg.as_multi_u64()?[0] as usize)),
        _ => Err(DeviceTreeError::WidthUnsupported)
    }
}