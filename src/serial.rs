/* diosix RV32G/RV64G hardware serial controller
 *
 * (c) Chris Williams, 2019.
 *
 * See LICENSE for usage and copying.
 */

use core::ptr::write_volatile;

/* serial port controller registers, relative to the base address */
const TXDATA: usize = 0x0;     /* write a byte here to transmit it over the port */
const TXCTRL: usize = 0x8;     /* transmission control register */

const TXCTRL_ENABLE: u32 = 1 << 0; /* bit 0 of TXCTRL = 1 to enable transmission */

/* define a standard serial port input/output device */
pub struct SerialPort
{
    base: usize /* base MMIO address of the serial port controller */
}

impl SerialPort
{
    /* create a new serial port
       => base_addr = serial controller's hardware base MMIO address
       <= serial port device object */
    pub fn new(base_addr: usize) -> SerialPort
    {
        /* enable tx by setting bit TXCTRL_ENABLE to 1 */
        unsafe { write_volatile((base_addr + TXCTRL) as *mut u32, TXCTRL_ENABLE); }

        SerialPort
        {
            base: base_addr
        }
    }

    /* return MMIO base address of the serial port */
    pub fn get_mmio_base(&self) -> usize { self.base }

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