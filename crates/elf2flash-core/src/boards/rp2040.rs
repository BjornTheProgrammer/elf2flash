use crate::boards::{BoardInfo, UsbDevice};

#[derive(Debug, Default, Clone)]
pub struct RP2040;

impl BoardInfo for RP2040 {
    fn is_device_board(&self, device: &UsbDevice) -> bool {
        if device.vendor_id != 0x2e8a {
            return false;
        }
        match device.product_id {
            0x0003 => true,
            _ => false,
        }
    }

    fn family_id(&self) -> u32 {
        0xe48bff56
    }

    fn board_name(&self) -> &'static str {
        "RP2040"
    }
}
