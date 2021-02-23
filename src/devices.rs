/* base hardware peripherals on RV32/64 systems
 *
 * discover hardware from a firmware-supplied device tree 
 * 
 * (c) Chris Williams, 2019-2020.
 *
 * See LICENSE for usage and copying.
 */
 
extern crate devicetree;
use devicetree::{DeviceTree, DeviceTreeBlob, DeviceTreeError, DeviceTreeProperty};

use super::serial;
use super::physmem;
use super::timer;
use super::errata;
use super::cpu;

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
    scheduler_timer: Option<timer::Timer>,      /* periodic timer for the scheduler */ 

    /* known errata we need to deal with */
    errata_known: u64,                          /* bitfield of errata we know about */
    errata_fixed: u64                           /* bitfield of errata we can fix */
}

impl Devices
{
    /* create a new Devices structure using the given device tree blob: the device tree
    binary is parsed to populate the structure with details of the system's base hardware
        => dtb = ptr to device tree blob to parse
        <= Some(Devices) if successful, or None for failure
    */
    pub fn new(dtb: &[u8]) -> Result<Devices, DeviceTreeError>
    {
        let blob = DeviceTreeBlob::from_slice(dtb)?;
        let parsed = blob.to_parsed()?;

        let (errata_known, errata_fixed) = errata::from_model(parsed.get_property(&format!("/"), &format!("model"))?.as_text()?);

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
                        Ok(t) =>
                        {
                            t.pin(); /* pin this timer for other platform code */
                            Some(t)
                        },
                        Err(_) => None /* would be nice to flag up error */
                    }
                }
                else
                {
                    None
                }
            },

            parsed,
            errata_known,
            errata_fixed
        };

        Ok(d)
    }

    /* write msg string out to the debug serial port */
    pub fn write_debug_string(&self, msg: &str)
    {
        if let Some(con) = &self.debug_console
        {
            con.write(msg);
        }
    }

    /* get a character from the debug serial port
       do not block waiting for a char to arrive.
       just check if a byte is waiting for us */
       pub fn read_debug_char(&self) -> Option<char>
       {
           if let Some(con) = &self.debug_console
           {
               if let Some(byte) = con.read()
               {
                   match byte
                   {
                       /* ignore null bytes */
                       0 => return None,
                       b => return Some(b as char)
                   }
               }
           }
           None
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

    /* return the timer's current value, or None if no timer
    this is a clock-on-the-wall timer in that its value always
    increases and never resets (though may rollover to 0) */
    pub fn scheduler_get_timer_now(&self) -> Option<timer::TimerValue>
    {
        if let Some(s) = self.scheduler_timer
        {
            return Some(s.get_now());
        }
        None
    }

    /* interrupt this CPU core with a tiemr IRQ after duration number
    of ticks or sub-seconds have passed */
    pub fn scheduler_timer_next_in(&self, duration: timer::TimerValue)
    {
        if let Some(s) = self.scheduler_timer
        {
            s.next_in(duration);
        }
    }

    /* get the target value of the next timer IRQ */
    pub fn scheduler_get_timer_next_at(&self) -> Option<timer::TimerValue>
    {
        if let Some(s) = self.scheduler_timer
        {
            return Some(s.get_next_at());
        }
        None
    }

    /* get the timer's frequency in Hz */
    pub fn scheduler_get_timer_frequency(&self) -> Option<u64>
    {
        if let Some(s) = self.scheduler_timer
        {
            return Some(s.get_frequency());
        }
        None
    }
    

    /* interrupt this CPU core when its timer values passes
    the target number of ticks or sub-seconds */
    pub fn scheduler_timer_at(&self, target: timer::TimerValue)
    {
        if let Some(s) = self.scheduler_timer
        {
            s.next_at(target);
        }
    }

    /* create a virtualized environment based on the host's peripherals for guest supervisors.
       => cpus = number of CPU cores in this virtual envuironment
          boot_cpu_id = ID of CPU core that can or will boot the system
          ram_base = base physical address of the environment's contiguous RAM area
          ram_size = number of bytes of the contiguous RAM area
       <= array of bytes containing the device tree blob for the environment,
          or None for failure */
    pub fn spawn_virtual_environment(&self, cpus: usize, boot_cpu_id: u32, ram_base: physmem::PhysMemBase, ram_size: physmem::PhysMemSize) -> Option<Vec<u8>>
    {
        let mut dt = DeviceTree::new();
        dt.edit_property(&format!("/"), &format!("#address-cells"), DeviceTreeProperty::UnsignedInt32(2));
        dt.edit_property(&format!("/"), &format!("#size-cells"), DeviceTreeProperty::UnsignedInt32(2));

        /* define the system memory's base physical address and size */
        dt.edit_property(&format!("/memory@{:x}", ram_base), &format!("reg"),
            DeviceTreeProperty::MultipleUnsignedInt64_64(vec!((ram_base as u64, ram_size as u64))));
        dt.edit_property(&format!("/memory@{:x}", ram_base), &format!("device_type"),
            DeviceTreeProperty::Text(format!("memory")));

        /* define the CPU cores */
        let cpu_root_path = format!("/cpus");
        dt.edit_property(&cpu_root_path, &format!("#address-cells"), DeviceTreeProperty::UnsignedInt32(1));
        dt.edit_property(&cpu_root_path, &format!("#size-cells"), DeviceTreeProperty::UnsignedInt32(0));

        match self.parsed.get_property(&format!("/cpus"), &format!("timebase-frequency"))
        {
            Ok(prop) => if let Ok(freq) = prop.as_u32()
            {
                dt.edit_property(&cpu_root_path, &format!("timebase-frequency"),
                    DeviceTreeProperty::UnsignedInt32(freq));
            },
            Err(_) => () /* TODO: should we guess the timebase frequency instead? */
        }

        for cpu in 0..cpus
        {
            let cpu_node_path = format!("{}/cpu@{}", &cpu_root_path, cpu);
            dt.edit_property(&cpu_node_path, &format!("device_type"), DeviceTreeProperty::Text(format!("cpu")));
            dt.edit_property(&cpu_node_path, &format!("reg"), DeviceTreeProperty::UnsignedInt32(cpu as u32));
            dt.edit_property(&cpu_node_path, &format!("status"), DeviceTreeProperty::Text(format!("okay")));
            dt.edit_property(&cpu_node_path, &format!("compatible"), DeviceTreeProperty::Text(format!("riscv")));
            match cpu::get_isa_width()
            {
                32 => dt.edit_property(&cpu_node_path, &format!("mmu-type"), DeviceTreeProperty::Text(format!("riscv,sv32"))),
                64 | 128 => dt.edit_property(&cpu_node_path, &format!("mmu-type"), DeviceTreeProperty::Text(format!("riscv,sv48"))),
                w => panic!("Cannot derive virtualized environment. Unsupported ISA width {}", w)
            }

            /* get the lower case ISA string */
            let isa = (cpu::CPUDescription).isa_to_string().to_lowercase();
            dt.edit_property(&cpu_node_path, &format!("riscv,isa"), DeviceTreeProperty::Text(isa));

            /* create an interrupt controller for this CPU core */
            let intc_node_path = format!("{}/interrupt-controller", &cpu_node_path);
            dt.edit_property(&intc_node_path, &format!("#interrupt-cells"), DeviceTreeProperty::UnsignedInt32(1));
            dt.edit_property(&intc_node_path, &format!("interrupt-controller"), DeviceTreeProperty::Empty);
            dt.edit_property(&intc_node_path, &format!("compatible"), DeviceTreeProperty::Text(format!("riscv,cpu-intc")));
        }

        /* direct console IO through the SBI interface, run OS in single-user mode */
        let chosen_node_path = format!("/chosen");
        dt.edit_property(&chosen_node_path, &format!("bootargs"), DeviceTreeProperty::Text(format!("console=hvc0")));

        dt.set_boot_cpu_id(boot_cpu_id);
        match dt.to_blob()
        {
            Ok(v) => Some(v),
            Err(_) => None
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

    /* the MMIO base address of the serial port is the first value in the reg list.
    the MMIO area size is the second value */
    let reg = match dt.get_property(path, &format!("reg"))
    {
        Ok(r) => r,
        Err(e) => return Err(e)
    };

    /* get the compatibility string for this controller.
    allows us and guest kernels to know how to talk to this serial port */
    let compat = match dt.get_property(path, &format!("compatible"))
    {
        Ok(c) => c,
        Err(c) => return Err(c)
    };

    match cells.address
    {
        1 => match serial::SerialPort::new(
                    reg.as_multi_u32()?[0] as usize, /* base addr */
                    reg.as_multi_u32()?[1] as usize, /* size */
                    &compat.as_text()?)
                    {
                        Some(sp) => Ok(sp),
                        None => Err(DeviceTreeError::DeviceFailure)
                    },
        2 => match serial::SerialPort::new(
                    reg.as_multi_u64()?[0] as usize, /* base addr */
                    reg.as_multi_u64()?[1] as usize, /* size */
                    &compat.as_text()?)
                    {
                        Some(sp) => Ok(sp),
                        None => Err(DeviceTreeError::DeviceFailure)
                    },
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