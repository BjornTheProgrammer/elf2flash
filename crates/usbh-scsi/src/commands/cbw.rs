use crate::commands::CommandBlock;

pub const CBW_SIGNATURE: u32 = 0x43425355;

#[repr(C, packed)]
#[allow(non_snake_case)]
pub struct Cbw {
    pub dCBWSignature: u32,
    pub dCBWTag: u32,
    pub dCBWDataTransferLength: u32,
    pub bmCBWFlags: u8,
    pub bCBWLUN: u8,
    pub bCBWCBLength: u8,
    pub CBWCB: [u8; 16],
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    In,
    Out,
}
impl Cbw {
    pub fn new<T: CommandBlock>(tag: u32, data_len: u32, direction: Direction, cmd: &T) -> Self {
        let cmd_bytes = cmd.to_bytes();
        assert!(cmd_bytes.len() <= 16, "Command block too long");

        Self {
            dCBWSignature: CBW_SIGNATURE,
            dCBWTag: tag,
            dCBWDataTransferLength: data_len,
            bmCBWFlags: match direction {
                Direction::In => 0x80,
                Direction::Out => 0x00,
            },
            bCBWLUN: 0,
            bCBWCBLength: cmd_bytes.len() as u8,
            CBWCB: cmd_bytes,
        }
    }

    /// Serialize into exactly 31 bytes (the CBW size)
    pub fn to_bytes(&self) -> [u8; 31] {
        let mut buf = [0u8; 31];

        // dCBWSignature (always 0x43425355)
        buf[0..4].copy_from_slice(&self.dCBWSignature.to_le_bytes());

        // dCBWTag
        buf[4..8].copy_from_slice(&self.dCBWTag.to_le_bytes());

        // dCBWDataTransferLength
        buf[8..12].copy_from_slice(&self.dCBWDataTransferLength.to_le_bytes());

        // bmCBWFlags
        buf[12] = self.bmCBWFlags;

        // bCBWLUN
        buf[13] = self.bCBWLUN;

        // bCBWCBLength
        buf[14] = self.bCBWCBLength;

        // CBWCB
        buf[15..31].copy_from_slice(&self.CBWCB);

        buf
    }
}
