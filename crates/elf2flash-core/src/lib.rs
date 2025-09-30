use std::{
    collections::HashSet,
    io::{Cursor, Read, Write},
};

use ::elf::{ElfBytes, ParseError, endian::AnyEndian};
use assert_into::AssertInto;
use log::debug;
use thiserror::Error;
use zerocopy::IntoBytes;

use crate::{
    address_range::AddressRangesFromElfError,
    boards::BoardInfo,
    elf::{get_page_fragments, realize_page},
    uf2::{
        UF2_FLAG_FAMILY_ID_PRESENT, UF2_MAGIC_END, UF2_MAGIC_START0, UF2_MAGIC_START1,
        Uf2BlockData, Uf2BlockFooter, Uf2BlockHeader,
    },
};

pub mod address_range;
pub mod boards;
pub mod elf;
pub mod uf2;

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

#[derive(Error, Debug)]
pub enum Elf2Uf2Error {
    #[error("Failed to get address ranges from elf")]
    AddressRangesError(#[from] AddressRangesFromElfError),
    #[error("Failed to parse elf file")]
    ElfParseError(#[from] ParseError),
    #[error("Failed to realize pages")]
    RealizePageError(#[from] std::io::Error),
    #[error("The input file has no memory pages")]
    InputFileNoMemoryPagesError,
}

/// Convert a file to a uf2 file. Give an input, and it generates an output. If you don't want to provide a family_id or reporter, then the family_id defaults to
/// the rp2040's family id. Just pass in the NoProgress struct to reporter you do not wish to have progress reporting.
///
/// # Examples
///
/// ```
/// use std::io::Cursor;
/// use elf2flash_core::{elf2uf2, boards, NoProgress};
///
/// log::set_max_level(log::LevelFilter::Debug);
/// let bytes_in = &include_bytes!("../tests/rp2040/hello_usb.elf")[..];
/// let mut bytes_out = Vec::new();
/// let board = boards::RP2040::default();
/// elf2uf2(bytes_in, &mut bytes_out, board, NoProgress).unwrap();
/// ```
pub fn elf2uf2(
    input: impl AsRef<[u8]>,
    mut output: impl Write,
    board: &dyn BoardInfo,
    mut reporter: impl ProgressReporter,
) -> Result<(), Elf2Uf2Error> {
    let input = input.as_ref();
    let file = ElfBytes::<AnyEndian>::minimal_parse(input)?;

    let page_size = board.page_size();
    let flash_sector_erase_size = board.flash_sector_erase_size();
    let family_id = board.family_id();

    let mut pages = get_page_fragments(&file, page_size);

    if pages.is_empty() {
        return Err(Elf2Uf2Error::InputFileNoMemoryPagesError);
    }

    let touched_sectors: HashSet<u64> = pages
        .keys()
        .map(|addr| addr / flash_sector_erase_size)
        .collect();

    let last_page_addr = *pages
        .last_key_value()
        .expect("Impossible error occurred since pages is garunteed to have a last page")
        .0;
    for sector in touched_sectors {
        let mut page = sector * flash_sector_erase_size;

        while page < (sector + 1) * flash_sector_erase_size {
            if page < last_page_addr && !pages.contains_key(&page) {
                pages.insert(page, Vec::new());
            }
            page += page_size as u64;
        }
    }

    let mut block_header = Uf2BlockHeader {
        magic_start0: UF2_MAGIC_START0,
        magic_start1: UF2_MAGIC_START1,
        flags: UF2_FLAG_FAMILY_ID_PRESENT,
        target_addr: 0,
        payload_size: page_size,
        block_no: 0,
        num_blocks: pages.len() as u32,
        file_size: family_id,
    };

    let mut block_data: Uf2BlockData = [0; 476];

    let block_footer = Uf2BlockFooter {
        magic_end: UF2_MAGIC_END,
    };

    log::debug!("Writing program");

    reporter.start(pages.len() * 512);

    let last_page_num = pages.len() - 1;

    for (page_num, (target_addr, fragments)) in pages.into_iter().enumerate() {
        block_header.target_addr = target_addr as u32;
        block_header.block_no = page_num.assert_into();

        debug!(
            "Page {} / {} {:#08x}",
            block_header.block_no as u32,
            block_header.num_blocks as u32,
            block_header.target_addr as u32
        );

        block_data.iter_mut().for_each(|v| *v = 0);

        realize_page(
            &mut Cursor::new(input),
            &fragments,
            &mut block_data,
            page_size,
        )?;

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

    #[test]
    pub fn hello_usb() {
        log::set_max_level(log::LevelFilter::Debug);
        let bytes_in = &include_bytes!("../tests/rp2040/hello_usb.elf")[..];
        let mut bytes_out = Vec::new();
        let board = boards::RP2040::default();
        elf2uf2(bytes_in, &mut bytes_out, &board, NoProgress).unwrap();

        assert_eq!(bytes_out, include_bytes!("../tests/rp2040/hello_usb.uf2"));
    }

    #[test]
    pub fn hello_serial() {
        log::set_max_level(log::LevelFilter::Debug);
        let bytes_in = &include_bytes!("../tests/rp2040/hello_serial.elf")[..];
        let mut bytes_out = Vec::new();
        let board = boards::RP2040::default();
        elf2uf2(bytes_in, &mut bytes_out, &board, NoProgress).unwrap();

        assert_eq!(
            bytes_out,
            include_bytes!("../tests/rp2040/hello_serial.uf2")
        );
    }
}
