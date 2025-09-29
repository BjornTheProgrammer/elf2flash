use bootsector::pio::ReadAt;

use crate::commands::{
    cbw::Direction,
    read_capacity::{ReadCapacity10Command, ReadCapacity10Data},
    read10::Read10Command,
    write10::Write10Command,
};
use std::{
    cell::RefCell,
    io::{self, Read as IoRead, Seek as IoSeek, SeekFrom, Write as IoWrite},
};

use crate::storage::{Opened, UsbMassStorage, UsbMassStorageReadWriteError};

// A std-IO-friendly wrapper over your open MSC device.
// (separate from your FatUsb that implements fatfs traits)
pub struct UsbBlockDevice {
    usb: RefCell<UsbMassStorage<Opened>>,
    block_size: u32,
    max_lba: u64,
    pos: u64,
}

impl UsbBlockDevice {
    pub fn new(mut usb: UsbMassStorage<Opened>) -> io::Result<Self> {
        // Query capacity to learn block size & last LBA
        let mut buf = [0u8; 8];
        let rc10 = ReadCapacity10Command::new(0);
        usb.execute_command(0x10, buf.len() as u32, Direction::In, &rc10, Some(&mut buf))
            .map_err(to_io_err)?;
        let cap = ReadCapacity10Data::parse(&buf).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "READ CAPACITY(10) parse failed")
        })?;

        let block_size = cap.block_length_bytes as u32;
        let max_lba = cap.last_logical_block_address as u64;

        Ok(Self {
            usb: RefCell::new(usb),
            block_size,
            max_lba,
            pos: 0,
        })
    }

    #[inline]
    fn disk_size(&self) -> u64 {
        (self.max_lba + 1) * self.block_size as u64
    }

    /// Write `count` consecutive blocks starting at `lba` from `buf`.
    ///
    /// - `count` must match `buf.len() / self.block_size`.
    /// - `buf.len()` must be exactly `count * block_size`.
    pub fn write_blocks(&mut self, tag: u32, lba: u32, count: u16, buf: &[u8]) -> io::Result<()> {
        let bs = self.block_size as usize;
        assert_eq!(buf.len(), bs * count as usize);

        // execute_command wants a &mut [u8] for outgoing payload
        let mut tmp = buf.to_vec();

        let cmd = Write10Command::new(0, lba, count);
        self.usb
            .get_mut()
            .execute_command(tag, buf.len() as u32, Direction::Out, &cmd, Some(&mut tmp))
            .map_err(to_io_err)
    }

    /// Read `count` consecutive blocks starting at `lba` into `buf`.
    ///
    /// - `count` must match `buf.len() / self.block_size`.
    /// - `buf.len()` must be exactly `count * block_size`.
    pub fn read_blocks(
        &mut self,
        tag: u32,
        lba: u32,
        count: u16,
        buf: &mut [u8],
    ) -> io::Result<()> {
        let bs = self.block_size as usize;
        assert_eq!(buf.len(), bs * count as usize);

        let cmd = Read10Command::new(0, lba, count);
        self.usb
            .get_mut()
            .execute_command(tag, buf.len() as u32, Direction::In, &cmd, Some(buf))
            .map_err(to_io_err)
    }

    fn read_at(&self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        // Clamp to disk size
        let remaining_on_disk = self.disk_size().saturating_sub(pos);
        if remaining_on_disk == 0 {
            return Ok(0);
        }
        let want = buf.len().min(remaining_on_disk as usize);

        let bs = self.block_size as usize;
        let start_lba = (pos / bs as u64) as u32;
        let offset_in_block = (pos % bs as u64) as usize;

        let total_bytes = want;
        let total_blocks = (offset_in_block + total_bytes + bs - 1) / bs;

        // Scratch buffer for all requested blocks
        let mut tmp = vec![0u8; total_blocks * bs];

        let read10 = Read10Command::new(0, start_lba, total_blocks as u16);

        self.usb
            .borrow_mut()
            .execute_command(
                0x21,
                tmp.len() as u32,
                Direction::In,
                &read10,
                Some(&mut tmp),
            )
            .map_err(to_io_err)?;

        buf[..want].copy_from_slice(&tmp[offset_in_block..offset_in_block + want]);
        Ok(want)
    }
}

fn to_io_err(e: UsbMassStorageReadWriteError) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e)
}

impl IoRead for UsbBlockDevice {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }

        // Clamp at end-of-disk
        let remaining_on_disk = self.disk_size().saturating_sub(self.pos);
        if remaining_on_disk == 0 {
            return Ok(0);
        }
        let want = out.len().min(remaining_on_disk as usize);

        let bs = self.block_size as usize;
        let start_lba = (self.pos / bs as u64) as u32;
        let offset_in_block = (self.pos % bs as u64) as usize;

        let total_bytes = want;
        let total_blocks = (offset_in_block + total_bytes + bs - 1) / bs;

        // Stage read into tmp
        let mut tmp = vec![0u8; total_blocks * bs];
        self.read_blocks(0x11, start_lba, total_blocks as u16, &mut tmp)?;

        out[..want].copy_from_slice(&tmp[offset_in_block..offset_in_block + want]);
        self.pos += want as u64;
        Ok(want)
    }
}

impl IoWrite for UsbBlockDevice {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        if src.is_empty() {
            return Ok(0);
        }

        // Clamp at end-of-disk
        let remaining_on_disk = self.disk_size().saturating_sub(self.pos);
        if remaining_on_disk == 0 {
            return Ok(0);
        }
        let want = src.len().min(remaining_on_disk as usize);

        let bs = self.block_size as usize;
        let mut cur_lba = (self.pos / bs as u64) as u32;
        let mut offset_in_block = (self.pos % bs as u64) as usize;

        let mut written = 0;
        while written < want {
            let chunk_left = want - written;

            // If we’re not aligned or won’t fill a whole block, do RMW one block
            if offset_in_block != 0 || chunk_left < bs {
                // read current block into temp
                let mut tmp = vec![0u8; bs];
                self.read_blocks(0x12, cur_lba, 1, &mut tmp)?;

                let copy_len = (bs - offset_in_block).min(chunk_left);
                tmp[offset_in_block..offset_in_block + copy_len]
                    .copy_from_slice(&src[written..written + copy_len]);

                self.write_blocks(0x13, cur_lba, 1, &tmp)?;

                written += copy_len;
                self.pos += copy_len as u64;
                cur_lba += 1;
                offset_in_block = 0;
                continue;
            }

            // We’re block-aligned and have at least a block to write — coalesce whole blocks
            let whole_blocks = chunk_left / bs;
            if whole_blocks == 0 {
                // less than one block remaining; RMW one block
                let mut tmp = vec![0u8; bs];
                self.read_blocks(0x14, cur_lba, 1, &mut tmp)?;

                let copy_len = chunk_left; // < bs
                tmp[..copy_len].copy_from_slice(&src[written..written + copy_len]);

                self.write_blocks(0x15, cur_lba, 1, &tmp)?;

                written += copy_len;
                self.pos += copy_len as u64;
                cur_lba += 1;
                offset_in_block = 0;
                continue;
            }

            // write N full blocks directly from src
            let byte_len = whole_blocks * bs;
            self.write_blocks(
                0x16,
                cur_lba,
                whole_blocks as u16,
                &src[written..written + byte_len],
            )?;

            written += byte_len;
            self.pos += byte_len as u64;
            cur_lba += whole_blocks as u32;
            offset_in_block = 0;
        }

        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl IoSeek for UsbBlockDevice {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let disk = self.disk_size() as i128;
        let cur = self.pos as i128;
        let dst: i128 = match pos {
            SeekFrom::Start(o) => o as i128,
            SeekFrom::End(off) => disk + off as i128,
            SeekFrom::Current(off) => cur + off as i128,
        };
        if dst < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before start",
            ));
        }
        let dst = dst.min(disk) as u64; // clamp to end
        self.pos = dst;
        Ok(self.pos)
    }
}

impl ReadAt for UsbBlockDevice {
    fn read_at(&self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.read_at(pos, buf)
    }
}
