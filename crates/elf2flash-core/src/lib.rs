use std::{collections::HashSet, error::Error, io::{Read, Write, Seek}};

use assert_into::AssertInto;
use log::debug;
use static_assertions::const_assert;
use zerocopy::IntoBytes;

use crate::{address_range::{FLASH_SECTOR_ERASE_SIZE, MAIN_RAM_END, MAIN_RAM_START, RP2040_ADDRESS_RANGES_FLASH, RP2040_ADDRESS_RANGES_RAM, XIP_SRAM_END, XIP_SRAM_START}, elf::{realize_page, AddressRangesExt, Elf32Header, PAGE_SIZE}, uf2::{Uf2BlockData, Uf2BlockFooter, Uf2BlockHeader, RP2040_FAMILY_ID, UF2_FLAG_FAMILY_ID_PRESENT, UF2_MAGIC_END, UF2_MAGIC_START0, UF2_MAGIC_START1}};

pub mod address_range;
pub mod elf;
pub mod uf2;
pub mod boards;

pub trait ProgressReporter {
    fn start(&mut self, total_bytes: usize);
    fn advance(&mut self, bytes: usize);
    fn finish(&mut self);
}

pub struct NoProgress;
impl ProgressReporter for NoProgress {
    fn start(&mut self, _total_bytes: usize) {}
    fn advance(&mut self, _bytes: usize) {}
    fn finish(&mut self) {}
}

/// Convert a file to a uf2 file. Give an input, and it generates an output. If you don't want to provide a family_id or reporter, then the family_id defaults to
/// the rp2040's family id. Just pass in the NoProgress struct to reporter you do not wish to have progress reporting.
///
/// # Examples
///
/// ```
/// use std::io::Cursor;
/// use elf2flash_core::{elf2uf2, NoProgress};
///
/// let bytes_in = std::io::Cursor::new(&include_bytes!("../tests/rp2040/hello_usb.elf")[..]);
/// let mut bytes_out = Vec::new();
/// elf2uf2(bytes_in, &mut bytes_out, None, NoProgress).unwrap();
/// ```
pub fn elf2uf2(mut input: impl Read + Seek, mut output: impl Write, family_id: Option<u32>, mut reporter: impl ProgressReporter) -> Result<(), Box<dyn Error>> {
    let eh = Elf32Header::from_read(&mut input)?;

    let entries = eh.read_elf32_ph_entries(&mut input)?;

    let ram_style = eh
        .is_ram_binary(&entries)
        .ok_or("entry point is not in mapped part of file".to_string())?;

    if ram_style {
        debug!("Detected RAM binary");
    } else {
        debug!("Detected FLASH binary");
    }

    let valid_ranges = if ram_style {
        RP2040_ADDRESS_RANGES_RAM
    } else {
        RP2040_ADDRESS_RANGES_FLASH
    };

    let mut pages = valid_ranges.check_elf32_ph_entries(&entries)?;

    if pages.is_empty() {
        return Err("The input file has no memory pages".into());
    }

    if ram_style {
        let mut expected_ep_main_ram = u32::MAX;
        let mut expected_ep_xip_sram = u32::MAX;

        #[allow(clippy::manual_range_contains)]
        pages.keys().copied().for_each(|addr| {
            if addr >= MAIN_RAM_START && addr <= MAIN_RAM_END {
                expected_ep_main_ram = expected_ep_main_ram.min(addr) | 0x1;
            } else if addr >= XIP_SRAM_START && addr < XIP_SRAM_END {
                expected_ep_xip_sram = expected_ep_xip_sram.min(addr) | 0x1;
            }
        });

        let expected_ep = if expected_ep_main_ram != u32::MAX {
            expected_ep_main_ram
        } else {
            expected_ep_xip_sram
        };

        if expected_ep == expected_ep_xip_sram {
            return Err("B0/B1 Boot ROM does not support direct entry into XIP_SRAM".into());
        } else if eh.entry != expected_ep {
            #[allow(clippy::unnecessary_cast)]
            return Err(format!(
                "A RAM binary should have an entry point at the beginning: {:#08x} (not {:#08x})",
                expected_ep, eh.entry as u32
            )
            .into());
        }
        const_assert!(0 == (MAIN_RAM_START & (PAGE_SIZE - 1)));

        // TODO: check vector table start up
        // currently don't require this as entry point is now at the start, we don't know where reset vector is
    } else {
        // Fill in empty dummy uf2 pages to align the binary to flash sectors (except for the last sector which we don't
        // need to pad, and choose not to to avoid making all SDK UF2s bigger)
        // That workaround is required because the bootrom uses the block number for erase sector calculations:
        // https://github.com/raspberrypi/pico-bootrom/blob/c09c7f08550e8a36fc38dc74f8873b9576de99eb/bootrom/virtual_disk.c#L205

        let touched_sectors: HashSet<u32> = pages
            .keys()
            .map(|addr| addr / FLASH_SECTOR_ERASE_SIZE)
            .collect();

        let last_page_addr = *pages.last_key_value().unwrap().0;
        for sector in touched_sectors {
            let mut page = sector * FLASH_SECTOR_ERASE_SIZE;

            while page < (sector + 1) * FLASH_SECTOR_ERASE_SIZE {
                if page < last_page_addr && !pages.contains_key(&page) {
                    pages.insert(page, Vec::new());
                }
                page += PAGE_SIZE;
            }
        }
    }

    let mut block_header = Uf2BlockHeader {
        magic_start0: UF2_MAGIC_START0,
        magic_start1: UF2_MAGIC_START1,
        flags: UF2_FLAG_FAMILY_ID_PRESENT,
        target_addr: 0,
        payload_size: PAGE_SIZE,
        block_no: 0,
        num_blocks: pages.len().assert_into(),
        file_size: family_id.unwrap_or(RP2040_FAMILY_ID),
    };

    let mut block_data: Uf2BlockData = [0; 476];

    let block_footer = Uf2BlockFooter {
        magic_end: UF2_MAGIC_END,
    };

    log::debug!("Writing program");

    reporter.start(pages.len() * 512);

    let last_page_num = pages.len() - 1;

    for (page_num, (target_addr, fragments)) in pages.into_iter().enumerate() {
        block_header.target_addr = target_addr;
        block_header.block_no = page_num.assert_into();

        debug!(
            "Page {} / {} {:#08x}",
            block_header.block_no as u32,
            block_header.num_blocks as u32,
            block_header.target_addr as u32
        );

        block_data.iter_mut().for_each(|v| *v = 0);

        realize_page(&mut input, &fragments, &mut block_data)?;

        output.write_all(block_header.as_bytes())?;
        output.write_all(block_data.as_bytes())?;
        output.write_all(block_footer.as_bytes())?;

        if page_num != last_page_num {
            reporter.advance(512);
        }
    }

    // Drop the output before the progress bar is allowd to finish
    drop(output);

    reporter.advance(512);

    reporter.finish();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    pub fn hello_usb() {
        log::set_max_level(log::LevelFilter::Debug);
        let bytes_in = io::Cursor::new(&include_bytes!("../tests/rp2040/hello_usb.elf")[..]);
        let mut bytes_out = Vec::new();
        elf2uf2(bytes_in, &mut bytes_out, None, NoProgress).unwrap();

        assert_eq!(bytes_out, include_bytes!("../tests/rp2040/hello_usb.uf2"));
    }

    #[test]
    pub fn hello_serial() {
        log::set_max_level(log::LevelFilter::Debug);
        let bytes_in = io::Cursor::new(&include_bytes!("../tests/rp2040/hello_serial.elf")[..]);
        let mut bytes_out = Vec::new();
        elf2uf2(bytes_in, &mut bytes_out, None, NoProgress).unwrap();

        assert_eq!(bytes_out, include_bytes!("../tests/rp2040/hello_serial.uf2"));
    }
}
