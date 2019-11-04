/* base hardware peripherals on RV32/64 systems
 *
 * (c) Chris Williams, 2019.
 *
 * See LICENSE for usage and copying.
 */

extern crate dtb;

use core::fmt;
use alloc::vec::Vec;
use super::serial;
use super::physmem;
use super::timer;

/* supported device types for the hypervisor */
#[derive(Hash, Eq, PartialEq, Debug, Clone, Copy)]
pub enum DeviceType
{
    PhysicalCPUCount,   /* number of physical processor CPU cores available */
    DebugConsole,       /* serial port for outputting debug */
    PhysicalRAM,        /* block(s) of physical memory available for software use */
    FixedTimer          /* a fixed periodic timer for scheduling workloads */
}

/* supported device structures for the hypervisor */
pub enum Device
{
    PhysicalCPUCount(usize),
    DebugConsole(serial::SerialPort),
    PhysicalRAM(Vec<physmem::RAMArea>),
    FixedTimer(timer::Timer)
}

/* supported return data from a device operation */
pub enum DeviceReturnData
{
    NoData
}

impl fmt::Debug for Device
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> core::fmt::Result
    {
        match self
        {
            Device::PhysicalCPUCount(c) => write!(f, "{} physical CPU cores", c),
            Device::DebugConsole(s) => write!(f, "Debug console (serial port) at 0x{:x}", s.get_mmio_base()),
            Device::PhysicalRAM(ram_areas) =>
            {
                for area in ram_areas
                {
                    match write!(f, "{} MiB of physical RAM available at 0x{:x}", area.size / 1024 / 1024, area.base)
                    {
                        Err(e) => return Err(e),
                        _ => ()                        
                    };
                }
                Ok(())
            },
            Device::FixedTimer(t) =>
            {
                let (base, freq) = (t.get_mmio_base(), t.get_frequency());
                write!(f, "Fixed {} Hz CLINT timer(s) (base 0x{:x})", freq, base)
            }
        }
    }
}

pub struct DeviceTreeIter
{
    device_tree: usize
}

impl DeviceTreeIter
{
    pub fn new(device_tree_buf: &u8) -> DeviceTreeIter
    {
        DeviceTreeIter
        {
            device_tree: 0
        }
    }
}

impl Iterator for DeviceTreeIter
{
    type Item = (DeviceType, Device);

    fn next(&mut self) -> Option<(DeviceType, Device)>
    {
        return None;
    }
}

pub fn enumerate(device_tree_buf: &u8) -> DeviceTreeIter
{    
    return DeviceTreeIter::new(device_tree_buf);
}