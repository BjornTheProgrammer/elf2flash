use crate::boards::{circuit_playground_bluefruit::CircuitPlaygroundBluefruit, rp2040::RP2040, rp2350::RP2350};

pub mod rp2040;
pub mod rp2350;
pub mod circuit_playground_bluefruit;

pub struct BoardIter {
    inner: std::vec::IntoIter<Box<dyn BoardInfo>>,
}

impl BoardIter {
    pub fn new() -> Self {
        Self {
            inner: vec![
                Box::new(RP2040::default()) as Box<dyn BoardInfo>,
                Box::new(RP2350::default()),
                Box::new(CircuitPlaygroundBluefruit::default()),
            ]
            .into_iter(),
        }
    }
}

impl Iterator for BoardIter {
    type Item = Box<dyn BoardInfo>;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

// define_boards!(RP2040, RP2350);

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct UsbVersion(pub u8, pub u8, pub u8);

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct UsbDevice {
    pub bus_number: u8,
    pub address: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub version: UsbVersion,
}

pub trait BoardInfo {
    /// Check if the board is connected to the specified UsbDevice
    fn is_device_board(&self, device: &UsbDevice) -> bool;

    /// Returns the proper family id to use for the uf2 device
    fn family_id(&self) -> u32;

    /// Optional, just sent to a sensible default of 256, as long as it is less than 512 - 32 it should be okay, but boards very, and so does the bootloader firmware
    fn page_size(&self) -> u32 {
        256
    }

    /// Optional, with a default erase size of 4096, this can be calculated by using
    fn flash_sector_erase_size(&self) -> u32 {
        4096
    }
}
