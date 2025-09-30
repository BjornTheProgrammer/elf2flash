use crate::commands::CommandBlock;

/// SCSI **READ(10)** command.
///
/// Requests the device to transfer one or more contiguous logical blocks
/// (sectors) starting from the given logical block address (LBA).
///
/// This is the primary way to read raw sectors from a USB Mass Storage device.
#[derive(Debug, Clone, Copy)]
pub struct Read10Command {
    /// Starting logical block address (sector index).
    pub logical_block_address: u32,
    /// Logical Unit Number (LUN). Usually `0` for single-LUN devices.
    pub logical_unit_number: u8,
    /// Number of contiguous blocks to read.
    pub transfer_length: u16,
}

impl Read10Command {
    /// Construct a new READ(10) command.
    ///
    /// - `logical_unit_number`: target LUN (usually 0).
    /// - `logical_block_address`: starting sector.
    /// - `transfer_length`: number of blocks to read.
    pub fn new(logical_unit_number: u8, logical_block_address: u32, transfer_length: u16) -> Self {
        Self {
            logical_block_address,
            logical_unit_number,
            transfer_length,
        }
    }
}

impl CommandBlock for Read10Command {
    fn to_bytes(&self) -> [u8; 16] {
        let mut cdb = [0u8; 16];
        cdb[0] = 0x28; // READ(10) opcode

        cdb[1] = (self.logical_unit_number & 0x07) << 5;

        // Logical Block Address (big-endian: MSB first)
        cdb[2] = (self.logical_block_address >> 24) as u8;
        cdb[3] = (self.logical_block_address >> 16) as u8;
        cdb[4] = (self.logical_block_address >> 8) as u8;
        cdb[5] = (self.logical_block_address & 0xFF) as u8;

        // Transfer Length (number of blocks, big-endian)
        cdb[7] = (self.transfer_length >> 8) as u8;
        cdb[8] = (self.transfer_length & 0xFF) as u8;

        // Other fields (LUN, reserved, control) left at 0
        cdb
    }

    fn len(&self) -> u8 {
        10 // READ(10) CDB is always 10 bytes
    }
}
