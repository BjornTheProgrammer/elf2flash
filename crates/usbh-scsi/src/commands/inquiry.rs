use crate::commands::CommandBlock;

/// SCSI INQUIRY command (CDB).
///
/// Requests basic device identification (vendor, product, revision,
/// and type). Typically the first command issued to discover what
/// kind of device is attached.
pub struct InquiryCommand {
    /// Allocation length: how many bytes the host expects back
    /// in the standard INQUIRY response.
    pub alloc_len: u8,
}

impl InquiryCommand {
    /// Construct a new `INQUIRY` command with the given expected response size.
    pub fn new(alloc_len: u8) -> Self {
        Self { alloc_len }
    }
}

impl CommandBlock for InquiryCommand {
    fn to_bytes(&self) -> [u8; 16] {
        let mut cdb = [0u8; 16];
        cdb[0] = 0x12; // INQUIRY opcode
        cdb[1] = 0x00; // EVPD = 0
        cdb[2] = 0x00; // page code
        cdb[3] = 0x00; // reserved
        cdb[4] = self.alloc_len; // allocation length
        cdb[5] = 0x00; // control
        cdb
    }

    fn len(&self) -> u8 {
        6 // INQUIRY always 6-byte CDB
    }
}

/// SCSI Peripheral Device Type field (from byte 0 of INQUIRY data).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeripheralDeviceType {
    /// Direct-access block device (e.g., disk).
    SbcDirectAccessDevice, // 0x00
    /// CD/DVD device.
    CdRomDevice, // 0x05
    /// Optical memory (e.g., MO disk).
    OpticalMemoryDevice, // 0x07
    /// RBC direct-access device.
    RbcDirectAccessDevice, // 0x0E
    /// Other or unrecognized value.
    OutOfScope(u8),
}

impl From<u8> for PeripheralDeviceType {
    fn from(value: u8) -> Self {
        match value {
            0x00 => PeripheralDeviceType::SbcDirectAccessDevice,
            0x05 => PeripheralDeviceType::CdRomDevice,
            0x07 => PeripheralDeviceType::OpticalMemoryDevice,
            0x0E => PeripheralDeviceType::RbcDirectAccessDevice,
            other => PeripheralDeviceType::OutOfScope(other),
        }
    }
}

/// Parsed standard INQUIRY response data (first 36 bytes).
///
/// Provides device type, removability flag, vendor/product/revision
/// identification fields.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct InquiryData {
    pub peripheral_device_type: PeripheralDeviceType,
    /// Whether or not the usb device is removable or not
    pub is_removable: bool,
    /// Additional Length field (byte 4).
    /// Indicates the number of bytes following byte 4 in the standard INQUIRY data.
    pub additional_length: u8,
    /// Optional ASCII vendor ID (8 bytes, space padded).
    pub vendor_identification: [u8; 8],
    /// Optional ASCII product ID (16 bytes, space padded).
    pub product_identification: [u8; 16],
    /// Optional ASCII product revision (4 bytes, space padded).
    pub product_revision_level: [u8; 4],
}

impl std::fmt::Debug for InquiryData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InquiryData")
            .field("peripheral_device_type", &self.peripheral_device_type)
            .field("is_removable", &self.is_removable)
            .field("additional_length", &self.additional_length)
            .field("vendor", &self.vendor())
            .field("product", &self.product())
            .field("revision", &self.revision())
            .finish()
    }
}

impl InquiryData {
    /// Parse a standard 36-byte INQUIRY response.
    ///
    /// Returns `None` if the buffer is shorter than 36 bytes.
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 36 {
            return None;
        }

        let peripheral_device_type = PeripheralDeviceType::from(buf[0] & 0x1F);
        let is_removable = buf[1] & 0x80 != 0;
        let additional_length = buf[4];

        let mut vendor_identification = [0u8; 8];
        vendor_identification.copy_from_slice(&buf[8..16]);

        let mut product_identification = [0u8; 16];
        product_identification.copy_from_slice(&buf[16..32]);

        let mut product_revision_level = [0u8; 4];
        product_revision_level.copy_from_slice(&buf[32..36]);

        Some(Self {
            peripheral_device_type,
            is_removable,
            additional_length,
            vendor_identification,
            product_identification,
            product_revision_level,
        })
    }

    /// Vendor ID string (trimmed ASCII).
    pub fn vendor(&self) -> String {
        String::from_utf8_lossy(&self.vendor_identification)
            .trim()
            .to_string()
    }

    /// Product ID string (trimmed ASCII).
    pub fn product(&self) -> String {
        String::from_utf8_lossy(&self.product_identification)
            .trim()
            .to_string()
    }

    /// Revision string (trimmed ASCII).
    pub fn revision(&self) -> String {
        String::from_utf8_lossy(&self.product_revision_level)
            .trim()
            .to_string()
    }
}
