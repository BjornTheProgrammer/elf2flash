use std::io::{Read, Seek, SeekFrom, Write};

use fatfs::FatType;
use rusb::{Device, GlobalContext};
use thiserror::Error;
use usbh_scsi::storage::{
    Closed, Opened, UsbMassStorage, UsbMassStorageError, UsbMassStorageReadWriteError,
};

pub use bootsector;
pub use rusb;

extern crate fatfs;

#[derive(Debug)]
pub struct StorageUsb {
    pub inner: StorageUsbInner,
    pub usb_device: Device<GlobalContext>,
}

#[derive(Debug)]
pub enum StorageUsbInner {
    Closed(UsbMassStorage<Closed>),
    Opened(UsbMassStorage<Opened>),
    ClosedDummy,
}

#[derive(Error, Debug)]
pub enum StorageUsbError {
    #[error("usb mass storage error")]
    UsbMassStorageError(#[from] UsbMassStorageError),
    #[error("failed to open as block device")]
    BlockDeviceOpenFail,
    #[error("listing partitions failed")]
    ListingPartitionFail,
}

impl StorageUsb {
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

#[derive(Debug, Clone)]
pub struct FatPartition {
    pub inner: bootsector::Partition,
    pub volume_id: u32,
    pub volume_label: String,
    pub fat_type: FatType,
    pub cluster_size: u32,
    pub first_byte: u64,
    pub length: u64,
}

impl FatPartition {
    pub fn list_partitions(usb: &mut StorageUsb) -> Result<Vec<Self>, StorageUsbError> {
        let opened = usb.open().unwrap();

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

pub struct PartitionView<D> {
    pub inner: D,
    pub start: u64,
    pub len: u64,
}

impl<D: Seek> PartitionView<D> {
    pub fn new(mut inner: D, start: u64, len: u64) -> Result<Self, FatError> {
        // Seek the underlying device to partition start so first reads work as expected.
        inner.seek(SeekFrom::Start(start)).map_err(FatError::from)?;
        Ok(Self { inner, start, len })
    }

    fn current_rel_pos(&mut self) -> Result<u64, std::io::Error> {
        let abs = self.inner.seek(SeekFrom::Current(0))?;
        Ok(abs.saturating_sub(self.start))
    }
}

impl<D> PartitionView<D> {
    fn clamp_rel(&self, rel: i128) -> u64 {
        let len = self.len as i128;
        rel.clamp(0, len) as u64
    }
}

impl<D: Read + Seek> Read for PartitionView<D> {
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

#[derive(Error, Debug)]
pub enum FatError {
    #[error("write interrupted")]
    Interrupted,
    #[error("unexpected end of file")]
    UnexpectedEof,
    #[error("zero write")]
    WriteZero,

    #[error("usb read/write failed: {0}")]
    UsbIo(#[from] UsbMassStorageReadWriteError),

    #[error("io error: {0}")]
    StdIo(#[from] std::io::Error),
}
