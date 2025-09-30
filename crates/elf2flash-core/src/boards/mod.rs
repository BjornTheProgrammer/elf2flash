mod circuit_playground_bluefruit;
mod rp2040;
mod rp2350;

pub use circuit_playground_bluefruit::CircuitPlaygroundBluefruit;
pub use rp2040::RP2040;
pub use rp2350::RP2350;
use thiserror::Error;

/// This is a helper struct, which allows you to iterate over every board defined
pub struct BoardIter {
    inner: std::vec::IntoIter<Box<dyn BoardInfo>>,
}

impl BoardIter {
    /// Creates a new BoardIter
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

    pub fn find_by_name(name: &str) -> Option<Box<dyn BoardInfo>> {
        for board in Self::new() {
            if board.board_name().eq_ignore_ascii_case(name) {
                return Some(board);
            }
        }
        None
    }
}

impl Iterator for BoardIter {
    type Item = Box<dyn BoardInfo>;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// This is the version of the firmware on the usb device
#[allow(unused)]
#[derive(Debug, Clone)]
pub struct UsbVersion(pub u8, pub u8, pub u8);

/// This is the usb device information from the usb device. It is possible to generate this information with something like
/// rusb
#[allow(unused)]
#[derive(Debug, Clone)]
pub struct UsbDevice {
    pub bus_number: u8,
    pub address: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub version: UsbVersion,
}

/// This trait helps by allowing for definitions of multiple different boards.
pub trait BoardInfo {
    /// Check if the board is connected to the specified UsbDevice
    fn is_device_board(&self, device: &UsbDevice) -> bool;

    /// Returns the proper family id to use for the uf2 device
    fn family_id(&self) -> u32;

    /// Optional, just sent to a sensible default of 256, as long as it is less than 512 - 32 it should be okay, but boards very, and so does the bootloader firmware
    fn page_size(&self) -> u32 {
        256
    }

    /// Optional, with a default erase size of 4096
    fn flash_sector_erase_size(&self) -> u64 {
        4096
    }

    /// Get the board's name
    fn board_name(&self) -> String;
}

/// A builder for the CustomBoard struct, which can be passed into the elf2uf2 function
pub struct CustomBoardBuilder {
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    family_id: Option<u32>,
    board_name: Option<String>,
    page_size: Option<u32>,
    flash_sector_erase_size: Option<u64>,
}

impl CustomBoardBuilder {
    pub fn new() -> Self {
        Self {
            vendor_id: None,
            product_id: None,
            family_id: None,
            board_name: None,
            page_size: None,
            flash_sector_erase_size: None,
        }
    }

    pub fn vendor_id(mut self, vendor_id: u16) -> Self {
        self.vendor_id = Some(vendor_id);
        self
    }

    pub fn product_id(mut self, product_id: u16) -> Self {
        self.product_id = Some(product_id);
        self
    }

    pub fn family_id(mut self, family_id: u32) -> Self {
        self.family_id = Some(family_id);
        self
    }

    pub fn board_name<S: Into<String>>(mut self, board_name: S) -> Self {
        self.board_name = Some(board_name.into());
        self
    }

    pub fn page_size(mut self, page_size: u32) -> Self {
        self.page_size = Some(page_size);
        self
    }

    pub fn flash_sector_erase_size(mut self, size: u64) -> Self {
        self.flash_sector_erase_size = Some(size);
        self
    }

    pub fn build(self) -> Result<CustomBoard, CustomBoardBuildError> {
        Ok(CustomBoard {
            vendor_id: self.vendor_id,
            product_id: self.product_id,
            family_id: self
                .family_id
                .ok_or(CustomBoardBuildError::FamilyIdRequired)?,
            board_name: self.board_name,
            page_size: self.page_size,
            flash_sector_erase_size: self.flash_sector_erase_size,
        })
    }
}

#[derive(Error, Debug)]
pub enum CustomBoardBuildError {
    #[error("family_id is required")]
    FamilyIdRequired,
    #[error("page_size is required")]
    PageSizeRequired,
    #[error("flash_sector_erase_size is required")]
    FlashSectorEraseSizeRequired,
}

/// A struct, which can be passed into the elf2uf2 function, this can be constructed via the CustomBoardBuilder struct.
pub struct CustomBoard {
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    family_id: u32,
    board_name: Option<String>,
    page_size: Option<u32>,
    flash_sector_erase_size: Option<u64>,
}

impl BoardInfo for CustomBoard {
    fn is_device_board(&self, device: &UsbDevice) -> bool {
        if let Some(vendor_id) = self.vendor_id
            && device.vendor_id != vendor_id
        {
            return false;
        }

        if let Some(vendor_id) = self.product_id
            && device.product_id != vendor_id
        {
            return false;
        }

        true
    }

    fn family_id(&self) -> u32 {
        self.family_id
    }

    fn board_name(&self) -> String {
        self.board_name.clone().unwrap_or("custom".to_string())
    }

    fn page_size(&self) -> u32 {
        self.page_size.unwrap_or(256)
    }

    fn flash_sector_erase_size(&self) -> u64 {
        self.flash_sector_erase_size.unwrap_or(4096)
    }
}
