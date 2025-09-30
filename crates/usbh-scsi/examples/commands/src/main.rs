use std::error::Error;

use usbh_scsi::commands::inquiry::InquiryData;
use usbh_scsi::commands::read_capacity::{ReadCapacity10Command, ReadCapacity10Data};
use usbh_scsi::commands::{cbw::Direction, inquiry::InquiryCommand, read10::Read10Command};
use usbh_scsi::storage::UsbMassStorage;

fn main() -> Result<(), Box<dyn Error>> {
    // List all available USB Mass Storage devices.
    let mut devices = UsbMassStorage::list()?;
    let Some(closed) = devices.pop() else {
        eprintln!("No USB mass storage devices found.");
        return Ok(());
    };

    // Open the first device.
    let mut dev = closed.open()?;

    let inquiry = InquiryCommand::new(0);
    let mut inquiry_buf = [0u8; 36];
    dev.execute_command(
        0, // logical unit number (LUN 0)
        inquiry_buf.len() as u32,
        Direction::In,
        &inquiry,
        Some(&mut inquiry_buf),
    )?;
    let inquiry_data = InquiryData::parse(&inquiry_buf).unwrap();
    println!(
        "Inquiry:\n    product: '{}'\n    vendor: '{}'\n    revision: '{}'",
        inquiry_data.product(),
        inquiry_data.vendor(),
        inquiry_data.revision()
    );

    let mut buf = [0u8; 8];
    let rc10 = ReadCapacity10Command::new(0);
    dev.execute_command(0x10, buf.len() as u32, Direction::In, &rc10, Some(&mut buf))?;
    let read_capacity_data = ReadCapacity10Data::parse(&buf).unwrap();

    println!(
        "\nCapacity: {} bytes, block size: {} bytes",
        read_capacity_data.total_capacity_bytes(),
        read_capacity_data.block_length_bytes,
    );

    // --- Read first 512 bytes (assuming block size = 512) ---
    let block_size = read_capacity_data.block_length_bytes;
    let mut block_buf = vec![0u8; block_size as usize];

    let read_cmd = Read10Command::new(0, 0, 1); // LUN 0, LBA 0, count 1 block
    dev.execute_command(
        0x20, // new tag for this command
        block_buf.len() as u32,
        Direction::In,
        &read_cmd,
        Some(&mut block_buf),
    )?;

    println!("\nFirst {} bytes from device:", block_buf.len());
    for (i, byte) in block_buf.iter().enumerate() {
        if i % 16 == 0 {
            print!("\n{:04x}: ", i);
        }
        print!("{:02x} ", byte);
    }
    println!();

    Ok(())
}
