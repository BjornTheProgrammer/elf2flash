use elf2flash_core::{
    ProgressReporter,
    boards::{BoardIter, CustomBoardBuilder},
    elf2uf2,
};
use env_logger::Env;
use log::Level;
use pbr::{ProgressBar, Units};
use std::{
    error::Error,
    fs::File,
    io::{Read, Stdout, Write},
};

use log::LevelFilter;

use clap::{Parser, ValueEnum};

use crate::commands::{convert::convert, deploy::deploy};

pub mod commands;
pub mod progress_bar;

#[derive(Copy, Clone, Debug, ValueEnum)]
enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Parser, Debug)]
#[clap(version, about, long_about = None, author = "Bjorn Beishline")]
struct Cli {
    /// Set the logging verbosity
    #[clap(short, long, value_enum, global = true, default_value_t = LogLevel::Info)]
    verbose: LogLevel,

    #[clap(subcommand)]
    command: Option<Command>,
}

#[derive(Parser, Debug)]
enum Command {
    /// Convert ELF to UF2 file on disk
    Convert {
        /// Input ELF file
        input: String,

        /// Output UF2 file
        output: String,

        /// Explicit board (rp2040, rp2350, circuit_playground_bluefruit, etc.)
        #[clap(short, long, value_parser = board_parser)]
        board: Option<String>,

        /// Override family ID
        #[clap(short, long, value_parser = num_parser)]
        family: Option<u32>,

        /// Flash erase sector size
        #[clap(short = 'e', long, value_parser = num_parser)]
        flash_sector_erase_size: Option<u64>,

        /// Page size
        #[clap(short, long, value_parser = num_parser)]
        page_size: Option<u32>,
    },
    /// Deploy ELF directly to a connected board
    Deploy {
        /// Input ELF file
        input: String,

        /// Same options as convert…
        #[clap(short, long, value_parser = board_parser)]
        board: Option<String>,

        /// Override family ID
        #[clap(short, long, value_parser = num_parser)]
        family: Option<u32>,

        /// Flash erase sector size
        #[clap(short = 'e', long, value_parser = num_parser)]
        flash_sector_erase_size: Option<u64>,

        /// Page size
        #[clap(short, long, value_parser = num_parser)]
        page_size: Option<u32>,

        /// Connect to serial after deploy
        #[clap(short, long)]
        serial: bool,

        /// Send termination message on Ctrl+C
        #[clap(short, long)]
        term: bool,
    },
}

fn board_parser(s: &str) -> Result<String, String> {
    if let Some(board) = BoardIter::find_by_name(s) {
        Ok(board.board_name().to_string())
    } else {
        Err(format!("Unknown board '{}'", s))
    }
}

// allow user to pass hex formatted numbers (typically the format used by family ids)
fn num_parser(s: &str) -> Result<u32, &'static str> {
    match s.get(0..2) {
        Some("0x") => u32::from_str_radix(&s[2..], 16).map_err(|_| "invalid hex number"),
        Some("0b") => u32::from_str_radix(&s[2..], 2).map_err(|_| "invalid binary number"),
        _ => s.parse::<u32>().map_err(|_| "invalid decimal number"),
    }
}

impl From<LogLevel> for LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => LevelFilter::Error,
            LogLevel::Warn => LevelFilter::Warn,
            LogLevel::Info => LevelFilter::Info,
            LogLevel::Debug => LevelFilter::Debug,
            LogLevel::Trace => LevelFilter::Trace,
            LogLevel::Off => LevelFilter::Off,
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    env_logger::Builder::from_env(Env::default())
        .filter_level(LevelFilter::from(cli.verbose))
        .format(|buf, record| {
            let level = record.level();
            if level == Level::Info {
                writeln!(buf, "{}", record.args())
            } else {
                writeln!(buf, "{}: {}", record.level(), record.args())
            }
        })
        .init();

    let command = match cli.command {
        Some(command) => command,
        None => return Ok(()),
    };

    match command {
        Command::Convert {
            input,
            output,
            board,
            family,
            flash_sector_erase_size,
            page_size,
        } => {
            return Ok(convert(
                input,
                output,
                board,
                family,
                flash_sector_erase_size,
                page_size,
            )?);
        }
        Command::Deploy {
            input,
            board,
            family,
            flash_sector_erase_size,
            page_size,
            serial,
            term,
        } => {
            return Ok(deploy(
                input,
                board,
                family,
                flash_sector_erase_size,
                page_size,
                serial,
                term,
            )?);
        }
    }
}
