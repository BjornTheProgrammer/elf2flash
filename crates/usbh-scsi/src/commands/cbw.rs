use crate::commands::CommandBlock;

/// Magic signature identifying a valid CBW (`'USBC'` little-endian).
pub const CBW_SIGNATURE: u32 = 0x43425355;

/// USB Mass Storage Bulk-Only Transport **Command Block Wrapper (CBW)**.
///
/// A CBW is a 31-byte structure sent from host to device over the
/// bulk-OUT endpoint. It wraps a SCSI command descriptor block (CDB)
/// together with transfer length, data direction, and a host-supplied
/// tag for matching responses.
#[repr(C, packed)]
#[allow(non_snake_case)]
pub struct Cbw {
    /// Must always be `0x43425355` (`'USBC'`).
    pub dCBWSignature: u32,
    /// Host-assigned tag echoed back in CSW (status).
    pub dCBWTag: u32,
    /// Number of data bytes expected in the data phase.
    pub dCBWDataTransferLength: u32,
    /// Direction flag: `0x80` = IN (device→host), `0x00` = OUT (host→device).
    pub bmCBWFlags: u8,
    /// Logical Unit Number (LUN). Usually `0` for single-LUN devices.
    pub bCBWLUN: u8,
    /// Length of the command block in bytes (1–16).
    pub bCBWCBLength: u8,
    /// Command Block (SCSI CDB), zero-padded to 16 bytes.
    pub CBWCB: [u8; 16],
}

/// Direction of data phase for a CBW.
#[derive(Debug, Clone, Copy)]
pub enum Direction {
    /// Device → Host transfer (e.g. READ).
    In,
    /// Host → Device transfer (e.g. WRITE).
    Out,
}

impl Cbw {
    /// Construct a new CBW for a given SCSI command.
    ///
    /// - `tag`: host-assigned identifier, echoed in the CSW.
    /// - `data_len`: number of bytes expected in the data phase.
    /// - `direction`: transfer direction.
    /// - `cmd`: the SCSI command implementing [`CommandBlock`].
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

    /// Serialize into exactly 31 bytes (the CBW wire format).
    ///
    /// This buffer is sent over the bulk-OUT endpoint prior to any data
    /// or status stage.
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
