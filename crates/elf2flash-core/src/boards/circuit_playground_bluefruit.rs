use crate::boards::{BoardInfo, UsbDevice};

/// This is the Circuit Playfround Bluefruit board
#[derive(Debug, Default, Clone)]
pub struct CircuitPlaygroundBluefruit;

impl BoardInfo for CircuitPlaygroundBluefruit {
    fn is_device_board(&self, device: &UsbDevice) -> bool {
        // https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/master/src/boards/circuitplayground_nrf52840/board.h
        if device.vendor_id != 0x239A {
            return false;
        }
        match device.product_id {
            0x0045 => true,
            _ => false,
        }
    }

    fn family_id(&self) -> u32 {
        0xada52840
    }

    fn board_name(&self) -> &'static str {
        "CircuitPlaygroundBluefruit"
    }
}
