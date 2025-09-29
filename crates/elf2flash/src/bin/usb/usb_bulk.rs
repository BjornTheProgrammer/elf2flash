use std::{alloc::Layout, time::Duration};

use rusb::{DeviceHandle, GlobalContext};
use scsi::{
    BufferPullable, BufferPushable, CommunicationChannel, ErrorCause, ScsiError,
    scsi::commands::{self, Command, Direction},
};

/// Holds a handle to the open device and the bulk endpoints.
pub struct BulkUsbChannel {
    handle: DeviceHandle<GlobalContext>,
    in_address: u8,
    in_max_packet_size: u16,
    out_address: u8,
    out_max_packet_size: u16,
    interface_number: u8,
    timeout: Duration,
}

impl Drop for BulkUsbChannel {
    fn drop(&mut self) {
        let _ = self.handle.release_interface(self.interface_number);
    }
}

impl BulkUsbChannel {
    pub fn new(
        handle: DeviceHandle<GlobalContext>,
        interface_number: u8,
        config_number: u8,
        in_address: u8,
        in_max_packet_size: u16,
        out_address: u8,
        out_max_packet_size: u16,
        timeout: Duration,
    ) -> Result<Self, anyhow::Error> {
        // Always do this first to avoid "Resource busy".
        handle.set_auto_detach_kernel_driver(true)?;

        // Make sure the right configuration is active.
        // (libusb usually sets config 1 by default, but don’t assume.)
        handle.set_active_configuration(config_number).ok();

        // MSC almost always uses alt setting 0.
        handle.claim_interface(interface_number)?;
        handle.set_alternate_setting(interface_number, 0).ok();

        // Clear any leftover halts from a previous session.
        handle.clear_halt(in_address).ok();
        handle.clear_halt(out_address).ok();

        Ok(Self {
            handle,
            in_address,
            in_max_packet_size,
            out_address,
            out_max_packet_size,
            interface_number,
            timeout,
        })
    }
}

impl CommunicationChannel for BulkUsbChannel {
    fn out_transfer<B: AsRef<[u8]>>(&mut self, bytes: B) -> Result<usize, ScsiError> {
        let data = bytes.as_ref();
        match self.handle.write_bulk(self.out_address, data, self.timeout) {
            Ok(n) => Ok(n),
            Err(rusb::Error::Pipe) => {
                // OUT stalled; clear halt(s) and surface as transport error (or retry once)
                let _ = self.handle.clear_halt(self.out_address);
                let _ = self.handle.clear_halt(self.in_address);
                println!("USB bulk OUT stalled");
                Err(ScsiError::from_cause(scsi::ErrorCause::UsbTransferError {
                    direction: scsi::UsbTransferDirection::Out,
                }))
            }
            Err(e) => {
                println!("USB bulk OUT error: {e}");
                Err(ScsiError::from_cause(scsi::ErrorCause::UsbTransferError {
                    direction: scsi::UsbTransferDirection::Out,
                }))
            }
        }
    }
    fn in_transfer<B: AsMut<[u8]>>(&mut self, mut buffer: B) -> Result<usize, ScsiError> {
        let buf = buffer.as_mut();
        match self.handle.read_bulk(self.in_address, buf, self.timeout) {
            Ok(n) => Ok(n),
            Err(rusb::Error::Pipe) => {
                // IN stalled; clear halt(s) and surface as transport error (or retry once)
                let _ = self.handle.clear_halt(self.in_address);
                let _ = self.handle.clear_halt(self.out_address);
                println!("USB bulk IN stalled");
                Err(ScsiError::from_cause(scsi::ErrorCause::UsbTransferError {
                    direction: scsi::UsbTransferDirection::In,
                }))
            }
            Err(e) => {
                println!("USB bulk IN error: {e}");
                Err(ScsiError::from_cause(scsi::ErrorCause::UsbTransferError {
                    direction: scsi::UsbTransferDirection::In,
                }))
            }
        }
    }
}

use std::mem::size_of;

// BOT packet sizes
const CBW_SIGNATURE: u32 = 0x43425355; // 'USBC'
const CSW_SIGNATURE: u32 = 0x53425355; // 'USBS'
const CBW_LEN: usize = 31;
const CSW_LEN: usize = 13;

pub fn send_scsi_inquiry(channel: &mut BulkUsbChannel) -> anyhow::Result<()> {
    let inquiry = commands::InquiryCommand::new(36);
    let mut cbw2 = [0u8; CBW_LEN];
    inquiry.push_to_buffer(&mut cbw2).unwrap();

    // Build a CBW (Command Block Wrapper)
    let tag: u32 = 0xdeadbeef; // arbitrary
    let data_len: u32 = 36; // expected INQUIRY response size
    let flags: u8 = 0x80; // 0x80 = device → host (IN transfer)
    let lun: u8 = 0; // LUN 0
    let cdb_len: u8 = 6; // INQUIRY command length

    // SCSI INQUIRY CDB: [opcode, evpd, page code, alloc len, control]
    let cdb: [u8; 16] = [
        0x12, // INQUIRY
        0x00, // EVPD = 0
        0x00, // page code
        0x00,
        data_len as u8, // allocation length (low byte only, 36 fits)
        0x00,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ];

    let mut cbw = [0u8; CBW_LEN];
    cbw[0..4].copy_from_slice(&CBW_SIGNATURE.to_le_bytes());
    cbw[4..8].copy_from_slice(&tag.to_le_bytes());
    cbw[8..12].copy_from_slice(&data_len.to_le_bytes());
    cbw[12] = flags;
    cbw[13] = lun;
    cbw[14] = cdb_len;
    cbw[15..31].copy_from_slice(&cdb);

    println!("our cbw: {:?}", cbw);
    println!("their cbw: {:?}", cbw2);

    // Send CBW
    channel.out_transfer(&cbw)?;

    // Read INQUIRY data
    let mut inquiry_buf = vec![0u8; data_len as usize];
    let n = channel.in_transfer(&mut inquiry_buf)?;
    println!("INQUIRY data ({} bytes): {:02x?}", n, &inquiry_buf[..n]);

    // Read CSW
    let mut csw = [0u8; CSW_LEN];
    let n = channel.in_transfer(&mut csw)?;
    if n != CSW_LEN {
        anyhow::bail!("CSW wrong length: {}", n);
    }
    if &csw[0..4] != &CSW_SIGNATURE.to_le_bytes() {
        anyhow::bail!("Bad CSW signature");
    }
    let status = csw[12];
    println!("CSW status: {}", status);

    Ok(())
}
