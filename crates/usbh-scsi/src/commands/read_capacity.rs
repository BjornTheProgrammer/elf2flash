use crate::commands::CommandBlock;

/// READ CAPACITY (10) command — returns 8 bytes of capacity data
#[derive(Debug, Clone, Copy)]
pub struct ReadCapacity10Command {
    pub logical_unit_number: u8,
}

impl ReadCapacity10Command {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadCapacity10Data {
    pub last_logical_block_address: u32,
    pub block_length_bytes: u32,
}

impl ReadCapacity10Data {
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

    pub fn total_capacity_bytes(&self) -> u64 {
        (self.last_logical_block_address as u64 + 1) * self.block_length_bytes as u64
    }
}
