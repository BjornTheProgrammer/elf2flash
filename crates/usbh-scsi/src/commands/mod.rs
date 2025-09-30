//! SCSI Command Blocks (CDBs).
//!
//! This module provides building blocks for constructing SCSI command
//! descriptor blocks (CDBs) to be sent over USB Mass Storage Bulk-Only
//! Transport.
//!
//! Each supported command (e.g. INQUIRY, READ(10), WRITE(10)) lives in
//! its own submodule. All implement the [`CommandBlock`] trait, which
//! allows them to be executed through [`UsbMassStorage::execute_command`](crate::storage::UsbMassStorage::execute_command).
//!
//! # Example
//! ```
//! use usbh_scsi::commands::{
//!     CommandBlock,
//!     inquiry::InquiryCommand,
//! };
//!
//! let cmd = InquiryCommand::new(0);
//! let bytes = cmd.to_bytes();
//! println!("CDB: {:02X?}", &bytes[..cmd.len() as usize]);
//! ```

pub mod cbw;
pub mod inquiry;
pub mod read10;
pub mod read_capacity;
pub mod write10;

/// Trait for any SCSI Command Block (CDB).
///
/// A `CommandBlock` encapsulates the fixed 16-byte array that represents a
/// SCSI command. All commands must specify their encoded bytes and effective
/// length (which may be shorter than 16).
///
/// Implementors of this trait are generally simple structs representing
/// a specific SCSI command (e.g. [`InquiryCommand`](crate::commands::inquiry::InquiryCommand),
/// [`Read10Command`](crate::commands::read10::Read10Command),
/// [`Write10Command`](crate::commands::write10::Write10Command)).
///
/// These are then passed into [`UsbMassStorage::execute_command`](crate::storage::UsbMassStorage::execute_command).
pub trait CommandBlock {
    /// Return the command descriptor block (CDB) as a fixed 16-byte array.
    ///
    /// Unused trailing bytes should be zeroed.
    fn to_bytes(&self) -> [u8; 16];

    /// Return the effective length of the command (number of meaningful bytes in the CDB).
    fn len(&self) -> u8;
}
