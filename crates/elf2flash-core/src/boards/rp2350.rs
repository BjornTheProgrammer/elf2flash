use crate::boards::{BoardInfo, UsbDevice};

#[derive(Debug, Default, Clone)]
pub struct RP2350;

impl BoardInfo for RP2350 {
    fn is_device_board(&self, device: &UsbDevice) -> bool {
        if device.vendor_id != 0x2e8a {
            return false;
        }
        match device.product_id {
            0x000f => true,
            _ => false,
        }
    }

    fn family_id(&self) -> u32 {
        // This is the rp2350 arm secure family id, should technically always be true if you held the bootsel button down and cycled power.
        0xe48bff59
    }

    fn board_name(&self) -> String {
        "rp2350".to_string()
    }
}
