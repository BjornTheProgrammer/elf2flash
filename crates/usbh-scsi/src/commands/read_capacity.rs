use crate::commands::CommandBlock;

/// SCSI **READ CAPACITY (10)** command.
///
/// Requests the device to report the capacity of the addressed logical unit.
/// The response is exactly 8 bytes:
///
/// - Bytes 0–3: Last Logical Block Address (LBA).
/// - Bytes 4–7: Block Length in bytes.
///
/// This is the standard way to discover a device’s sector size and total size
/// in bytes, and is typically issued once after `INQUIRY`.
#[derive(Debug, Clone, Copy)]
pub struct ReadCapacity10Command {
    /// Logical Unit Number (LUN). Usually `0` for single-LUN devices.
    pub logical_unit_number: u8,
}

impl ReadCapacity10Command {
    /// Construct a new `READ CAPACITY (10)` command for a given LUN.
    pub fn new(logical_unit_number: u8) -> Self {
        Self {
            logical_unit_number,
        }
    }
}

impl CommandBlock for ReadCapacity10Command {
    fn to_bytes(&self) -> [u8; 16] {
        let mut cdb = [0u8; 16];
        cdb[0] = 0x25; // READ CAPACITY (10) opcode

        // Byte 1: LUN in the upper 3 bits (bits 7–5)
        cdb[1] = (self.logical_unit_number & 0x07) << 5;

        // All other fields are reserved and left 0
        cdb
    }

    fn len(&self) -> u8 {
        10 // READ CAPACITY (10) uses a 10-byte CDB
    }
}

/// Parsed response to a **READ CAPACITY (10)** command (8 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadCapacity10Data {
    /// Address of the last logical block (zero-based).
    ///
    /// For example, if this is `999`, the device has 1000 blocks.
    pub last_logical_block_address: u32,
    /// Block size in bytes (e.g. `512`).
    pub block_length_bytes: u32,
}

impl ReadCapacity10Data {
    /// Parse the standard 8-byte READ CAPACITY (10) response buffer.
    ///
    /// Returns `None` if the buffer is shorter than 8 bytes.
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 8 {
            return None;
        }

        let last_logical_block_address = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let block_length_bytes = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);

        Some(Self {
            last_logical_block_address,
            block_length_bytes,
        })
    }

    /// Compute the total capacity of the device in bytes.
    ///
    /// ```
    /// # use usbh_scsi::commands::read_capacity::ReadCapacity10Data;
    /// let data = ReadCapacity10Data {
    ///     last_logical_block_address: 999,
    ///     block_length_bytes: 512,
    /// };
    /// assert_eq!(data.total_capacity_bytes(), 512_000);
    /// ```
    pub fn total_capacity_bytes(&self) -> u64 {
        (self.last_logical_block_address as u64 + 1) * self.block_length_bytes as u64
    }
}
