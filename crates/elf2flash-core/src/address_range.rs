use elf::{ElfBytes, abi::PT_LOAD, endian::EndianParse};
use thiserror::Error;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AddressRangeType {
    /// May have contents
    Contents,
    /// Must be uninitialized
    NoContents,
    /// will be ignored
    Ignore,
}

#[derive(Copy, Clone, Debug)]
pub struct AddressRange {
    pub typ: AddressRangeType,
    pub to: u64,
    pub from: u64,
}

impl AddressRange {
    pub const fn new(from: u64, to: u64, typ: AddressRangeType) -> Self {
        Self { typ, to, from }
    }
}

impl Default for AddressRange {
    fn default() -> Self {
        Self {
            typ: AddressRangeType::Ignore,
            to: 0,
            from: 0,
        }
    }
}

#[derive(Error, Debug)]
pub enum AddressRangesFromElfError {
    #[error("No segments in ELF")]
    NoSegments,
}

pub fn address_ranges_from_elf<E: EndianParse>(
    file: &ElfBytes<'_, E>,
) -> Result<Vec<AddressRange>, AddressRangesFromElfError> {
    let segments = file
        .segments()
        .ok_or(AddressRangesFromElfError::NoSegments)?;

    let mut ranges = Vec::new();

    for seg in segments {
        if seg.p_type != PT_LOAD || seg.p_memsz == 0 {
            continue;
        }

        let start = seg.p_paddr;
        let end = start + seg.p_memsz;

        if seg.p_filesz > 0 {
            // initialized contents
            ranges.push(AddressRange::new(
                start,
                start + seg.p_filesz,
                AddressRangeType::Contents,
            ));
        }

        if seg.p_memsz > seg.p_filesz {
            // uninitialized (BSS)
            ranges.push(AddressRange::new(
                start + seg.p_filesz,
                end,
                AddressRangeType::NoContents,
            ));
        }
    }

    Ok(ranges)
}
