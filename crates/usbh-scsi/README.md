# USBh-SCSI

A userspace library for sending **SCSI commands** to USB Mass Storage devices
via the **Bulk-Only Transport (BOT)** protocol.

Unlike traditional approaches, this crate does **not** depend on the operating
system’s block device or filesystem layers. Instead, it provides a pure-Rust
API for constructing and executing standard SCSI command blocks directly over
USB, making it portable across all platforms supported by [`rusb`].

## Goals

- Cross-platform support (Linux, macOS, Windows, etc. via [`rusb`]).
- Easy construction and execution of SCSI commands such as `INQUIRY`,
  `READ CAPACITY (10)`, `READ(10)`, and `WRITE(10)`.
- Clean abstractions for both raw transport and block-level access.

## Core Modules

- [`commands`] — strongly-typed definitions of SCSI commands and the
  [`CommandBlock`] trait for generating Command Descriptor Blocks (CDBs).
- [`storage`] — device discovery, opening/closing devices, bulk I/O, and
  SCSI command execution over USB BOT. Includes [`UsbBlockDevice`] for
  sector-oriented reads/writes.

## Usage

Add to your `Cargo.toml`:

```toml
cargo add usbh-scsi
````

## Example

```rust,no_run
use usbh_scsi::storage::UsbMassStorage;
use usbh_scsi::commands::inquiry::InquiryCommand;
use usbh_scsi::commands::cbw::Direction;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Enumerate all attached MSC devices
    let mut devices = UsbMassStorage::list()?;
    if let Some(closed) = devices.pop() {
        // Open the first one
        let mut dev = closed.open()?;

        // Send an INQUIRY command
        let cmd = InquiryCommand::new(0);
        let mut buf = [0u8; 36];
        dev.execute_command(1, buf.len() as u32, Direction::In, &cmd, Some(&mut buf))?;

        println!("INQUIRY data: {:?}", &buf);
    }
    Ok(())
}
```

## When to Use

* Use **`usbh-scsi`** if you want **raw SCSI access** to USB devices
  (e.g. discovering capacity, issuing reads/writes, or building custom tooling).
* Use **[`usbh-fatfs`](https://crates.io/crates/usbh-fatfs)** if you want a
  **filesystem-aware** interface for working directly with FAT partitions on
  USB devices.

## Supported Platforms

Works on any platform supported by [`rusb`].

[`rusb`]: https://docs.rs/rusb
[`commands`]: https://docs.rs/usbh-scsi/latest/usbh_scsi/commands/
[`CommandBlock`]: https://docs.rs/usbh-scsi/latest/usbh_scsi/commands/trait.CommandBlock.html
[`storage`]: https://docs.rs/usbh-scsi/latest/usbh_scsi/storage/
[`UsbBlockDevice`]: https://docs.rs/usbh-scsi/latest/usbh_scsi/storage/block_device/struct.UsbBlockDevice.html
