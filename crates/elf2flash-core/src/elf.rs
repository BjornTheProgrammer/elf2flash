use crate::address_range::{
    self, AddressRange, AddressRangeType, AddressRangesFromElfError, address_ranges_from_elf,
};
use assert_into::AssertInto;
use elf::{ElfBytes, abi::PT_LOAD, endian::EndianParse};
use log::debug;
use std::{
    cmp::min,
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom},
};

#[derive(Copy, Clone, Debug, Default)]
pub struct PageFragment {
    pub file_offset: u64,
    pub page_offset: u64,
    pub bytes: u64,
}

pub fn realize_page(
    input: &mut (impl Read + Seek),
    fragments: &[PageFragment],
    buf: &mut [u8],
    page_size: u32,
) -> Result<(), std::io::Error> {
    assert!(buf.len() >= page_size.assert_into());

    for frag in fragments {
        assert!(
            frag.page_offset < page_size as u64
                && frag.page_offset + frag.bytes <= page_size as u64
        );

        input.seek(SeekFrom::Start(frag.file_offset.assert_into()))?;

        input.read_exact(
            &mut buf[frag.page_offset.assert_into()..(frag.page_offset + frag.bytes).assert_into()],
        )?;
    }

    Ok(())
}

pub fn get_page_fragments<E: EndianParse>(
    file: &ElfBytes<E>,
    page_size: u32,
) -> Result<BTreeMap<u64, Vec<PageFragment>>, AddressRangesFromElfError> {
    let ranges = address_ranges_from_elf(&file)?;

    let mut pages = BTreeMap::<u64, Vec<PageFragment>>::new();

    for segment in file.segments().expect("Segments should exist in elf") {
        if segment.p_type == PT_LOAD && segment.p_memsz > 0 {
            let mapped_size = min(segment.p_filesz, segment.p_memsz);

            if mapped_size > 0 {
                let ar = ranges.as_slice().check_address_range(
                    segment.p_paddr,
                    segment.p_vaddr,
                    mapped_size,
                    false,
                )?;

                if ar.typ != AddressRangeType::Contents {
                    debug!("ignored");
                    continue;
                }

                let mut addr = segment.p_paddr;
                let mut remaining = mapped_size;
                let mut file_offset = segment.p_offset;

                while remaining > 0 {
                    let off = addr & (page_size - 1) as u64;
                    let len = min(remaining, page_size as u64 - off);

                    // list of fragments
                    let fragments = pages.entry(addr - off).or_default();

                    // note if filesz is zero, we want zero init which is handled because the
                    // statement above creates an empty page fragment list
                    // check overlap with any existing fragments
                    for fragment in fragments.iter() {
                        if (off < fragment.page_offset + fragment.bytes)
                            != ((off + len) <= fragment.page_offset)
                        {
                            panic!("In memory segments overlap");
                        }
                    }
                    fragments.push(PageFragment {
                        file_offset,
                        page_offset: off,
                        bytes: len,
                    });
                    addr += len;
                    file_offset += len;
                    remaining -= len;
                }
                if segment.p_memsz > segment.p_filesz {
                    // we have some uninitialized data too
                    ranges.as_slice().check_address_range(
                        segment.p_paddr + segment.p_filesz,
                        segment.p_vaddr + segment.p_filesz,
                        segment.p_memsz - segment.p_filesz,
                        true,
                    )?;
                }
            }
        }
    }

    Ok(pages)
}

pub trait AddressRangesExt<'a>: IntoIterator<Item = &'a AddressRange> + Clone {
    fn range_for(&self, addr: u64) -> Option<&'a AddressRange> {
        self.clone()
            .into_iter()
            .find(|r| r.from <= addr && r.to > addr)
    }

    fn check_address_range(
        &self,
        addr: u64,
        vaddr: u64,
        size: u64,
        uninitialized: bool,
    ) -> Result<AddressRange, AddressRangesFromElfError> {
        for range in self.clone().into_iter() {
            if range.from <= addr && range.to >= addr + size {
                if range.typ == address_range::AddressRangeType::NoContents && !uninitialized {
                    return Err(
                        AddressRangesFromElfError::MemoryContentsForUninitializedMemory(addr),
                    );
                }

                debug!(
                    "{} segment {:#08x}->{:#08x} ({:#08x}->{:#08x})",
                    if uninitialized {
                        "Uninitialized"
                    } else {
                        "Mapped"
                    },
                    addr,
                    addr + size,
                    vaddr,
                    vaddr + size
                );
                return Ok(*range);
            }
        }
        Err(AddressRangesFromElfError::MemorySegmentInvalidForDevice(
            addr,
            addr + size,
        ))
    }
}

impl<'a, T> AddressRangesExt<'a> for T where T: IntoIterator<Item = &'a AddressRange> + Clone {}
