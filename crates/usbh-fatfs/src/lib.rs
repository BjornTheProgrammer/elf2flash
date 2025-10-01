#![doc = include_str!("../README.md")]

use std::io::{Read, Seek, SeekFrom, Write};

use fatfs::FatType;
use rusb::{Device, GlobalContext};
use thiserror::Error;
use usbh_scsi::storage::{
    Closed, Opened, UsbMassStorage, UsbMassStorageError, UsbMassStorageReadWriteError,
};

/// Re-export of the `bootsector` crate for partition parsing.
pub use bootsector;
/// Re-export of the `fatfs` crate for FAT filesystem operations.
pub use fatfs;
/// Re-export of the `rusb` crate for raw USB device handling.
pub use rusb;

/// Represents a USB mass-storage device connected to the system.
///
/// Holds both the USB device handle (`rusb::Device`) and
/// the current state of its storage interface (`StorageUsbInner`).
#[derive(Debug)]
pub struct StorageUsb {
    pub inner: StorageUsbInner,
    pub usb_device: Device<GlobalContext>,
}

/// Represents the state of a `StorageUsb` device.
///
/// - `Closed`: The device is detected but not yet opened for I/O.
/// - `Opened`: The device is ready for block-level access.
/// - `ClosedDummy`: Temporary placeholder state during transitions.
#[derive(Debug)]
pub enum StorageUsbInner {
    Closed(UsbMassStorage<Closed>),
    Opened(UsbMassStorage<Opened>),
    ClosedDummy,
}

/// Errors that can occur when working with [`StorageUsb`] or partitions.
#[derive(Error, Debug)]
pub enum StorageUsbError {
    /// Wrapper around `UsbMassStorageError` (device communication issues).
    #[error("usb mass storage error")]
    UsbMassStorageError(#[from] UsbMassStorageError),

    /// Failed to open a block device interface.
    #[error("failed to open as block device")]
    BlockDeviceOpenFail,

    /// Partition listing failed (invalid or unreadable partition table).
    #[error("listing partitions failed")]
    ListingPartitionFail,
}

impl StorageUsb {
    /// List all connected USB mass-storage devices and wrap them in [`StorageUsb`].
    ///
    /// Returns a vector of `StorageUsb` instances, all starting in the `Closed` state.
    pub fn list_usbs() -> Result<Vec<Self>, StorageUsbError> {
        let usbs: Vec<_> = UsbMassStorage::list()?
            .into_iter()
            .map(|usb| {
                let device = usb.device.clone();

                Self {
                    inner: StorageUsbInner::Closed(usb),
                    usb_device: device,
                }
            })
            .collect();

        Ok(usbs)
    }

    /// Open the USB mass-storage device for I/O.
    ///
    /// If the device is already open, it will simply return the existing `Opened` instance.
    ///
    /// Returns a mutable reference to the `UsbMassStorage<Opened>` object for performing block I/O.
    pub fn open(&mut self) -> Result<&mut UsbMassStorage<Opened>, StorageUsbError> {
        // Take ownership safely by swapping with None
        let inner = std::mem::replace(&mut self.inner, StorageUsbInner::ClosedDummy);
        self.inner = match inner {
            StorageUsbInner::Closed(closed) => StorageUsbInner::Opened(closed.open()?),
            opened @ StorageUsbInner::Opened(_) => opened,
            _ => unreachable!(),
        };

        match &mut self.inner {
            StorageUsbInner::Opened(opened) => Ok(opened),
            _ => unreachable!(),
        }
    }
}

/// Represents a FAT partition discovered on a USB mass-storage device.
#[derive(Debug, Clone)]
pub struct FatPartition {
    /// Underlying raw partition information from `bootsector`.
    pub inner: bootsector::Partition,
    /// Volume ID of the FAT filesystem.
    pub volume_id: u32,
    /// Volume label string.
    pub volume_label: String,
    /// FAT type (e.g., FAT12, FAT16, FAT32).
    pub fat_type: FatType,
    /// Cluster size in bytes.
    pub cluster_size: u32,
    /// Byte offset of the partition start on the device.
    pub first_byte: u64,
    /// Length of the partition in bytes.
    pub length: u64,
}

impl FatPartition {
    /// List FAT partitions on a given USB storage device.
    ///
    /// Attempts to:
    /// 1. Open the device.
    /// 2. Parse its partition table.
    /// 3. Mount each partition as a FAT filesystem.
    ///
    /// Returns only valid FAT partitions (others are skipped).
    pub fn list_partitions(usb: &mut StorageUsb) -> Result<Vec<Self>, StorageUsbError> {
        let opened = usb.open()?;

        let mut block_device = opened
            .block_device()
            .map_err(|_| StorageUsbError::BlockDeviceOpenFail)?;

        let partitions =
            bootsector::list_partitions(&block_device, &bootsector::Options::default())
                .map_err(|_| StorageUsbError::ListingPartitionFail)?;

        let mut results = Vec::new();

        for partition in partitions {
            let first_byte = partition.first_byte;
            let length = partition.len;
            let view = PartitionView {
                inner: &mut block_device,
                start: first_byte,
                len: length,
            };

            let fs = match fatfs::FileSystem::new(view, fatfs::FsOptions::new()) {
                Ok(fs) => fs,
                Err(_) => continue,
            };

            results.push(Self {
                inner: partition,
                volume_id: fs.volume_id(),
                volume_label: fs.volume_label(),
                fat_type: fs.fat_type(),
                cluster_size: fs.cluster_size(),
                first_byte: first_byte,
                length: length,
            })
        }

        Ok(results)
    }
}

/// Provides a "window" into a block device, restricted to a single partition.
///
/// Wraps a seekable/readable/writable device and clamps all operations
/// so they cannot escape the defined partition region.
pub struct PartitionView<D> {
    /// The underlying device (e.g., USB block device).
    pub inner: D,
    /// Start offset of the partition in bytes.
    pub start: u64,
    /// Length of the partition in bytes.
    pub len: u64,
}

impl<D: Seek> PartitionView<D> {
    /// Create a new `PartitionView` wrapping a device.
    ///
    /// Seeks the device to the start of the partition immediately.
    pub fn new(mut inner: D, start: u64, len: u64) -> Result<Self, FatError> {
        // Seek the underlying device to partition start so first reads work as expected.
        inner.seek(SeekFrom::Start(start)).map_err(FatError::from)?;
        Ok(Self { inner, start, len })
    }

    /// Return the current relative position within the partition.
    ///
    /// Always non-negative and less than or equal to `len`.
    fn current_rel_pos(&mut self) -> Result<u64, std::io::Error> {
        let abs = self.inner.seek(SeekFrom::Current(0))?;
        Ok(abs.saturating_sub(self.start))
    }
}

impl<D> PartitionView<D> {
    /// Clamp a relative offset into the valid partition range `[0, len]`.
    fn clamp_rel(&self, rel: i128) -> u64 {
        let len = self.len as i128;
        rel.clamp(0, len) as u64
    }
}

impl<D: Read + Seek> Read for PartitionView<D> {
    /// Read data from within the partition without crossing its boundaries.
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // Ensure we never read past partition end
        let cur_rel = self.current_rel_pos()?;
        if cur_rel >= self.len {
            return Ok(0);
        }
        let max_here = (self.len - cur_rel) as usize;
        let want = buf.len().min(max_here);

        // Issue read — underlying device is already positioned absolutely
        let n = self.inner.read(&mut buf[..want])?;
        Ok(n)
    }
}

impl<D: Write + Seek> Write for PartitionView<D> {
    /// Write data to the partition without crossing its boundaries.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // Bound writes to the partition
        let cur_rel = self.current_rel_pos()?;
        if cur_rel >= self.len {
            return Ok(0);
        }
        let max_here = (self.len - cur_rel) as usize;
        let want = buf.len().min(max_here);

        let n = self.inner.write(&buf[..want])?;
        if n == 0 && want != 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "WriteZero"));
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<D: Seek> Seek for PartitionView<D> {
    /// Seek to a new relative offset within the partition.
    ///
    /// Guarantees that the position never moves outside `[0, len]`.
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        // Compute desired RELATIVE offset within [0, len]
        let rel_target: u64 = match pos {
            SeekFrom::Start(o) => self.clamp_rel(o as i128),
            SeekFrom::End(off) => {
                let rel = self.len as i128 + off as i128;
                self.clamp_rel(rel)
            }
            SeekFrom::Current(off) => {
                let cur_rel = self.current_rel_pos()? as i128;
                self.clamp_rel(cur_rel + off as i128)
            }
        };

        // Convert to absolute on the underlying device and seek.
        let abs = self.start + rel_target;
        let _ = self.inner.seek(SeekFrom::Start(abs));
        Ok(rel_target)
    }
}

/// Errors that can occur when reading/writing FAT partitions.
#[derive(Error, Debug)]
pub enum FatError {
    /// Write was interrupted.
    #[error("write interrupted")]
    Interrupted,

    /// Unexpected end of file during read.
    #[error("unexpected end of file")]
    UnexpectedEof,

    /// Attempted to write zero bytes unexpectedly.
    #[error("zero write")]
    WriteZero,

    /// USB-level I/O error during read/write.
    #[error("usb read/write failed: {0}")]
    UsbIo(#[from] UsbMassStorageReadWriteError),

    /// Generic I/O error from the standard library.
    #[error("io error: {0}")]
    StdIo(#[from] std::io::Error),
}
