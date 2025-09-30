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

use clap::Parser;

use crate::deploy_usb::{deploy_to_usb, get_plugged_in_boards, list_uf2_partitions};

pub mod deploy_usb;

#[derive(Parser, Debug, Default)]
#[clap(version, about, long_about = None, author = "Bjorn Beishline")]
struct Opts {
    /// Verbose
    #[clap(short, long)]
    verbose: bool,

    /// Deploy to any connected pico
    #[clap(short, long)]
    deploy: bool,

    /// Connect to serial after deploy
    #[clap(short, long)]
    serial: bool,

    /// Send termination message (b"elf2flash-term\r\n") to the device on ctrl+c
    #[clap(short, long)]
    term: bool,

    /// Select family ID for UF2. See https://github.com/microsoft/uf2/blob/master/utils/uf2families.json for list.
    #[clap(short, long, value_parser = num_parser)]
    family: Option<u32>,

    /// How many sectors that should be erasaed
    #[clap(short = 'e', long, value_parser = num_parser)]
    flash_sector_erase_size: Option<u64>,

    /// Page size of the uf2 device
    #[clap(short, long, value_parser = num_parser)]
    page_size: Option<u32>,

    /// Explicitly select board (rp2040, rp2350, circuit_playground_bluefruit, etc.)
    #[clap(short, long, value_parser = board_parser)]
    board: Option<String>,

    /// Input file
    input: String,

    /// Output file
    output: Option<String>,
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

struct ProgressBarReporter {
    pb: ProgressBar<Stdout>,
}

impl ProgressReporter for ProgressBarReporter {
    fn start(&mut self, total_bytes: usize) {
        self.pb.total = total_bytes as u64;
        self.pb.set_units(Units::Bytes);
    }

    fn advance(&mut self, bytes: usize) {
        self.pb.add(bytes as u64);
    }

    fn finish(&mut self) {
        self.pb.finish();
    }
}

impl ProgressBarReporter {
    pub fn new() -> Self {
        Self {
            pb: ProgressBar::new(0),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let options = Opts::parse();

    if options.verbose {
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(Env::default().default_filter_or("info"))
            .format(|buf, record| {
                let level = record.level();
                if level == Level::Info {
                    writeln!(buf, "{}", record.args())
                } else {
                    writeln!(buf, "{}: {}", record.level(), record.args())
                }
            })
            .init();
    }

    let serial_ports_before = serialport::available_ports()?;

    let input = &options.input;
    log::info!("Getting input file from {:?}", input);

    let mut input = File::open(input)?;
    let mut buf = Vec::new();
    input.read_to_end(&mut buf)?;
    let input = buf;

    log::info!("Getting plugged in boards");

    let plugged_in_boards = get_plugged_in_boards()?;

    if plugged_in_boards.is_empty() {
        log::info!("No recognized board plugged in");
        log::info!("Defaulting to search any fatfs system");
    } else {
        log::info!("Found board(s):");
        for board in &plugged_in_boards {
            let board = board.1.as_ref();
            log::info!(
                "    board: {} (family id: {:#x})",
                board.board_name(),
                board.family_id(),
            )
        }
    }

    for board in plugged_in_boards {
        let (_usb, board, mut storage_usb) = board;
        let partitions = list_uf2_partitions(board.as_ref(), &mut storage_usb).unwrap();

        let mut output = Vec::new();

        let mut custom_board = CustomBoardBuilder::new()
            .board_name(board.board_name())
            .family_id(options.family.unwrap_or(board.family_id()))
            .flash_sector_erase_size(
                options
                    .flash_sector_erase_size
                    .unwrap_or(board.flash_sector_erase_size()),
            )
            .page_size(options.page_size.unwrap_or(board.page_size()));

        if let Some(family_id) = options.family {
            custom_board = custom_board.family_id(family_id);
        }

        let custom_board = custom_board.build().unwrap();

        log::info!("Converting elf to uf2 file");

        elf2uf2(
            &input,
            &mut output,
            &custom_board,
            ProgressBarReporter::new(),
        )?;

        // New line after progress bar
        println!();

        for partition in partitions {
            deploy_to_usb(
                &output,
                &partition,
                &custom_board,
                &mut storage_usb,
                ProgressBarReporter::new(),
            )
            .unwrap();
        }
    }

    if options.serial {
        use std::process;
        use std::sync::{Arc, Mutex};
        use std::time::Duration;
        use std::{io, thread};

        let mut counter = 0;

        println!("Looking for pico serial...");

        let serial_port_info = 'find_loop: loop {
            for port in serialport::available_ports()? {
                if !serial_ports_before.contains(&port) {
                    println!("Found pico serial on {}", &port.port_name);
                    break 'find_loop Some(port);
                }
            }

            counter += 1;

            if counter == 100 {
                break None;
            }

            thread::sleep(Duration::from_millis(200));
        };

        if let Some(serial_port_info) = serial_port_info {
            for _ in 0..100 {
                if let Ok(port) = serialport::new(&serial_port_info.port_name, 115200)
                    .timeout(Duration::from_millis(100))
                    .flow_control(serialport::FlowControl::None)
                    .open()
                {
                    let port = Arc::new(Mutex::new(port));

                    let handler = {
                        let port = port.clone();
                        move || {
                            let mut port = port.lock().unwrap();
                            port.write_all(b"elf2flash-term\r\n").ok();
                            port.flush().ok();
                            process::exit(0);
                        }
                    };

                    if options.term {
                        ctrlc::set_handler(handler.clone()).expect("Error setting Ctrl-C handler");
                    }

                    let data_terminal_ready_succeeded = {
                        let mut port = port.lock().unwrap();
                        port.write_data_terminal_ready(true).is_ok()
                    };
                    if data_terminal_ready_succeeded {
                        let mut serial_buf = [0; 1024];
                        loop {
                            let read = {
                                let mut port = port.lock().unwrap();
                                port.read(&mut serial_buf)
                            };

                            match read {
                                Ok(t) => {
                                    use std::io::Write;

                                    io::stdout().write_all(&serial_buf[..t])?;
                                    io::stdout().flush()?;
                                }
                                Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
                                Err(e) if e.kind() == io::ErrorKind::Interrupted => {
                                    if options.term {
                                        handler();
                                    }
                                    return Err(e.into());
                                }
                                Err(e) => return Err(e.into()),
                            }
                        }
                    }
                }

                thread::sleep(Duration::from_millis(200));
            }
        }
    }

    Ok(())
}
