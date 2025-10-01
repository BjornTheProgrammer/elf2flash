# USBh-FatFS

A userspace library for interacting directly with FAT file systems on USB
mass-storage devices.

Unlike traditional approaches, this crate does **not** require mounting
the USB drive through the operating system. Instead, it uses [`rusb`] for
USB access and [`fatfs`] for working with FAT partitions directly in
userspace.

## Advantages

- **No OS mounting required**
  Work directly with USB storage devices without relying on the operating
  system’s block device or filesystem layers. This avoids mount permissions,
  auto-mount races, and OS-specific device paths.

- **Cross-platform**
  Works uniformly across Linux, macOS, and Windows (anywhere [`rusb`] is
  supported). No platform-specific filesystem APIs are needed.

- **Safe partition access**
  [`PartitionView`] provides a restricted window into a specific partition,
  ensuring reads and writes cannot escape its boundaries.

- **Filesystem control in userspace**
  Full control over how and when partitions are read/written, useful for
  embedded tooling, automated flashing utilities, or sandboxed environments.

- **Integrates with `usbh-scsi`**
  Built on top of [`usbh-scsi`], giving access to both high-level FAT
  operations and low-level SCSI commands in the same ecosystem.


## Supported Platforms

Works on any OS that [`rusb`] supports (Linux, macOS, Windows, etc.).

## Core Types

- [`StorageUsb`]: Represents a physical USB mass-storage device. Can be
  enumerated via [`StorageUsb::list_usbs`] and opened for block I/O.
- [`FatPartition`]: A parsed FAT partition on a device. Provides metadata
  such as volume label, FAT type, and cluster size.
- [`PartitionView`]: A safe "window" into a block device that restricts
  reads/writes to a single partition’s byte range. Used when creating a
  [`fatfs::FileSystem`] instance.

Together, these abstractions make it possible to safely:
1. Detect USB storage devices.
2. Parse their partition tables.
3. Mount FAT partitions entirely in userspace.

## Example

```rust
use usbh_fatfs::{
    FatPartition, PartitionView, StorageUsb,
    fatfs::{FileSystem, FsOptions},
};

fn main() {
    // Enumerate all connected USB mass-storage devices.
    let usbs = StorageUsb::list_usbs().unwrap();
    println!("usbs: {:?}", usbs);

    for mut usb in usbs {
        // Parse FAT partitions on this device.
        let partitions = FatPartition::list_partitions(&mut usb).unwrap();
        println!("partitions: {:?}", partitions);

        for partition in partitions {
            // Open device for block access.
            let opened = usb.open().unwrap();
            let mut block_device = opened.block_device().unwrap();

            // Restrict I/O to the partition boundaries.
            let part_view = PartitionView::new(&mut block_device, partition.first_byte, partition.length).unwrap();

            // Mount the FAT filesystem in userspace.
            let fatfs = FileSystem::new(part_view, FsOptions::new()).unwrap();

            println!("Listing root dir on volume:");
            for item in fatfs.root_dir().iter() {
                let item = item.unwrap();
                println!("    {}", item.file_name());
            }
        }
    }
}
```

## See Also

- Full examples are available in the repository:
  <https://github.com/BjornTheProgrammer/elf2flash/tree/main/crates/usbh-fatfs/examples>

## Crates Used
- [`rusb`] — USB device enumeration and communication.
- [`fatfs`] — Pure Rust FAT filesystem implementation.
- [`bootsector`] — Partition table parsing.
- [`usbh-scsi`] — USB Bulk-Only Transport and SCSI protocol support.
