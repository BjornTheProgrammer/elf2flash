use std::io::SeekFrom;

use usbh_scsi::storage::block_device::UsbBlockDevice;

extern crate fatfs;

pub struct PartitionView<'a> {
    inner: &'a mut UsbBlockDevice,
    start: u64,
    len: u64,
}

impl<'a> PartitionView<'a> {
    pub fn new(inner: &'a mut UsbBlockDevice, start: u64, len: u64) -> Result<Self, FatError> {
        // Seek the underlying device to partition start so first reads work as expected.
        inner.seek(SeekFrom::Start(start)).map_err(FatError::from)?;
        Ok(Self { inner, start, len })
    }

    fn clamp_rel(&self, rel: i128) -> u64 {
        let len = self.len as i128;
        rel.clamp(0, len) as u64
    }

    fn current_rel_pos(&mut self) -> Result<u64, FatError> {
        let abs = self
            .inner
            .seek(SeekFrom::Current(0))
            .map_err(FatError::from)?;
        Ok(abs.saturating_sub(self.start))
    }
}

impl<'a> fatfs::IoBase for PartitionView<'a> {
    type Error = FatError;
}

impl<'a> FatRead for PartitionView<'a> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
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
        let n = self.inner.read(&mut buf[..want]).map_err(FatError::from)?;
        Ok(n)
    }
}

impl<'a> FatWrite for PartitionView<'a> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
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

        let n = self.inner.write(&buf[..want]).map_err(FatError::from)?;
        if n == 0 && want != 0 {
            return Err(FatError::WriteZero);
        }
        Ok(n)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush().map_err(FatError::from)
    }
}

impl<'a> FatSeek for PartitionView<'a> {
    fn seek(&mut self, pos: FatSeekFrom) -> Result<u64, Self::Error> {
        // Compute desired RELATIVE offset within [0, len]
        let rel_target: u64 = match pos {
            FatSeekFrom::Start(o) => self.clamp_rel(o as i128),
            FatSeekFrom::End(off) => {
                let rel = self.len as i128 + off as i128;
                self.clamp_rel(rel)
            }
            FatSeekFrom::Current(off) => {
                let cur_rel = self.current_rel_pos()? as i128;
                self.clamp_rel(cur_rel + off as i128)
            }
        };

        // Convert to absolute on the underlying device and seek.
        let abs = self.start + rel_target;
        let _ = self
            .inner
            .seek(SeekFrom::Start(abs))
            .map_err(FatError::from)?;
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

impl IoError for FatError {
    fn is_interrupted(&self) -> bool {
        match self {
            FatError::Interrupted => true,
            _ => false,
        }
    }

    fn new_unexpected_eof_error() -> Self {
        FatError::UnexpectedEof
    }

    fn new_write_zero_error() -> Self {
        FatError::WriteZero
    }
}
