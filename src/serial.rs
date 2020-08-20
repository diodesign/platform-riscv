/* diosix RV32G/RV64G hardware serial controller
 *
 * (c) Chris Williams, 2019-2020.
 *
 * See LICENSE for usage and copying.
 */

use alloc::string::String;
use core::ptr::write_volatile;

/* serial port controller registers, relative to the base address */
const TXDATA: usize = 0x0;     /* write a byte here to transmit it over the port */

//const TXCTRL: usize = 0x8;     /* transmission control register */
//const TXCTRL_ENABLE: u32 = 1 << 0; /* bit 0 of TXCTRL = 1 to enable transmission */

/* define a standard serial port input/output device */
#[derive(Clone, Debug)]
pub struct SerialPort
{
    base: usize, /* base MMIO address of the serial port controller */
    size: usize, /* MMIO address space size of the serial port controller */
    compat: String /* string describing the hardware this is compatible with */
}

impl SerialPort
{
    /* create a new serial port
       => base_addr = serial controller's hardware base MMIO address
          size = serial controller's MMIO address space size in bytes
       <= serial port device object */
    pub fn new(base_addr: usize, size: usize, compat: &String) -> SerialPort
    {
        /* enable tx by setting bit TXCTRL_ENABLE to 1 */
        // unsafe { write_volatile((base_addr + TXCTRL) as *mut u32, TXCTRL_ENABLE); }

        SerialPort
        {
            base: base_addr,
            size: size,
            compat: compat.clone()
        }
    }

    /* return information about this serial port */
    pub fn get_mmio_base(&self) -> usize { self.base }
    pub fn get_mmio_size(&self) -> usize { self.size }
    pub fn get_compatibility(&self) -> &String { &self.compat }

    /* write the string msg to the serial port */
    pub fn write(&self, msg: &str)
    {
        for byte in msg.bytes()
        {
            self.write_byte(byte);
        }
    }

    /* write byte to_write out to the serial port */
    #[inline(always)]
    pub fn write_byte(&self, to_write: u8)
    {
        unsafe { write_volatile((self.base + TXDATA) as *mut u8, to_write); }
    }

    /* read a byte from the serial port, or None for no byte to read */
    #[inline(always)]
    pub fn read_byte(&self) -> Option<u8>
    {
        None
    }
}
