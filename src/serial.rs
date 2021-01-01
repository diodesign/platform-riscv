/* diosix RV32G/RV64G hardware serial port abstraction
 *
 * This creates a generic serial port that calls down
 * to a hardware-specific implementation selected by the
 * compatibility string
 * 
 * (c) Chris Williams, 2019-2020.
 *
 * See LICENSE for usage and copying.
 */

use alloc::string::String;
use mmio_16550_uart;

/* supported serial port controllers */
#[derive(Debug)]
enum Controllers
{
    NS16550a(mmio_16550_uart::UART)
}

/* define a standard serial port input/output device */
#[derive(Debug)]
pub struct SerialPort
{
    base: usize,
    size: usize,
    compat: String,
    chip: Controllers
}

impl SerialPort
{
    /* create a new serial port, if a driver exists for it
       => base = serial controller's hardware base MMIO address
          size = serial controller's MMIO address space size in bytes
          compat = comma-seperated string of devices this port is compatible with
       <= serial port device object, or None for error */
    pub fn new(base: usize, size: usize, compat: &String) -> Option<SerialPort>
    {
        let compat_str = compat.as_str();
        if compat_str.contains("16550a") == true
        {
            if let Ok(uart) = mmio_16550_uart::UART::new(base)
            {
                /* reject MMIO areas that are too small */
                if uart.size() > size
                {
                    return None;
                }

                return Some(SerialPort
                {
                    base, size, compat: compat.clone(),
                    chip: Controllers::NS16550a(uart)
                });
            }
            else
            {
                /* faild to create serial controller */
                return None;
            }
        }

        /* failed to find compatible controller */
        return None;
    }

    /* return information about this serial port */
    pub fn get_mmio_base(&self) -> usize { self.base }
    pub fn get_mmio_size(&self) -> usize { self.size }
    pub fn get_compatibility(&self) -> &String { &self.compat }

    /* write the string msg to the serial port
       <= true if successful, false if not */
    pub fn write(&self, msg: &str) -> bool
    {
        for byte in msg.bytes()
        {
            match &self.chip
            {
                Controllers::NS16550a(c) => match c.send_byte(byte)
                {
                    Ok(_) => (),
                    Err(_) => return false
                }
            }
        }

        true
    }

    /* read in a byte from the serial port */
    pub fn read(&self) -> Option<u8>
    {
        match &self.chip
        {
            Controllers::NS16550a(c) => match c.read_byte()
            {
                Ok(b) => Some(b),
                Err(_) => return None
            }   
        }
    }
}
