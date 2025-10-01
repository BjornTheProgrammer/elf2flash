use anyhow::{Result, anyhow};
use elf2flash_core::{
    boards::{BoardIter, CustomBoardBuilder},
    elf2uf2,
};
use std::{
    fs::File,
    io::{BufWriter, Read},
};

use crate::progress_bar::ProgressBarReporter;

pub fn convert(
    input: String,
    output: String,
    board: Option<String>,
    family: Option<u32>,
    flash_sector_erase_size: Option<u64>,
    page_size: Option<u32>,
) -> Result<()> {
    log::info!("Reading ELF file from {input:?}");

    // Read ELF into memory
    let mut input_file = File::open(&input)?;
    let mut buf = Vec::new();
    input_file.read_to_end(&mut buf)?;
    let input = buf;

    // Base builder
    let mut builder = CustomBoardBuilder::new();

    if let Some(board_name) = board {
        log::info!("Looking up board definition for {board_name}");

        let Some(base) = BoardIter::new().find(|b| b.board_name() == board_name) else {
            return Err(anyhow!("Unknown board: {board_name}"));
        };

        // Fill defaults from known board
        builder = builder
            .board_name(base.board_name())
            .family_id(base.family_id())
            .flash_sector_erase_size(base.flash_sector_erase_size())
            .page_size(base.page_size());
    }

    // Apply CLI overrides (always win over defaults)
    if let Some(fam) = family {
        builder = builder.family_id(fam);
    }
    if let Some(erase) = flash_sector_erase_size {
        builder = builder.flash_sector_erase_size(erase);
    }
    if let Some(p) = page_size {
        builder = builder.page_size(p);
    }

    // Require at least family_id in some form
    let custom_board = builder
        .build()
        .map_err(|_| anyhow!("Must provide --board or --family"))?;

    log::info!("Converting ELF → UF2");

    let output_file = File::create(&output)?;
    let mut writer = BufWriter::new(output_file);

    elf2uf2(
        &input,
        &mut writer,
        &custom_board,
        ProgressBarReporter::new(),
    )?;

    log::info!("Wrote UF2 to {output:?}");
    Ok(())
}
