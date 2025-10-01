use std::{fs::File, io::Read};

use anyhow::Result;
use elf2flash_core::{
    boards::{BoardIter, CustomBoardBuilder},
    elf2uf2,
};

use crate::{
    commands::deploy::to_usb::{deploy_to_usb, get_plugged_in_boards, list_uf2_partitions},
    progress_bar::ProgressBarReporter,
};

pub mod to_usb;

pub fn deploy(
    input: String,
    board: Option<String>,
    family: Option<u32>,
    flash_sector_erase_size: Option<u64>,
    page_size: Option<u32>,
    serial: bool,
    term: bool,
) -> Result<()> {
    let serial_ports_before = serialport::available_ports()?;

    log::info!("Getting input file from {:?}", input);

    let mut input = File::open(input)?;
    let mut buf = Vec::new();
    input.read_to_end(&mut buf)?;
    let input = buf;

    log::info!("Getting plugged in boards");

    let plugged_in_boards = get_plugged_in_boards()?;

    if plugged_in_boards.is_empty() {
        log::warn!("No uf2 devices found.");
        return Ok(());
    } else {
        log::info!("Found board(s):");
        for board in &plugged_in_boards {
            if let Some(board) = &board.1 {
                log::info!(
                    "    board: {} (family id: {:#x})",
                    board.board_name(),
                    board.family_id(),
                )
            } else {
                log::info!("    unorganized uf2 device");
            }
        }
    }

    for plugged_in_board in plugged_in_boards {
        let (_usb, plugged_in_board, mut storage_usb) = plugged_in_board;
        let custom_board = if let Some(board) = plugged_in_board {
            CustomBoardBuilder::new()
                .board_name(board.board_name())
                .family_id(family.unwrap_or(board.family_id()))
                .flash_sector_erase_size(
                    flash_sector_erase_size.unwrap_or(board.flash_sector_erase_size()),
                )
                .page_size(page_size.unwrap_or(board.page_size()))
        } else if let Some(ref board) = board {
            let board = BoardIter::new()
                .into_iter()
                .find(|b| &b.board_name() == board)
                .expect("Should be impossible for unrecognized board to appear here");

            CustomBoardBuilder::new()
                .board_name(board.board_name())
                .family_id(family.unwrap_or(board.family_id()))
                .flash_sector_erase_size(
                    flash_sector_erase_size.unwrap_or(board.flash_sector_erase_size()),
                )
                .page_size(page_size.unwrap_or(board.page_size()))
        } else {
            let family = match family {
                Some(family) => family,
                None => {
                    log::info!("Cannot flash to generic uf2 device without a family id specified");
                    continue;
                }
            };
            let mut board = CustomBoardBuilder::new()
                .board_name("generic_uf2")
                .family_id(family);

            if let Some(flash_sector_erase_size) = flash_sector_erase_size {
                board = board.flash_sector_erase_size(flash_sector_erase_size);
            }

            if let Some(page_size) = page_size {
                board = board.page_size(page_size);
            }

            board
        };

        let custom_board = custom_board
            .build()
            .expect("Should be able to build custom boarod");

        let partitions = match list_uf2_partitions(&custom_board, &mut storage_usb) {
            Ok(partitions) => partitions,
            Err(_err) => continue,
        };

        let mut output = Vec::new();

        log::info!("Converting elf to uf2 file");

        elf2uf2(
            &input,
            &mut output,
            &custom_board,
            ProgressBarReporter::new(),
        )?;

        // New line after progress bar
        log::info!("\n");

        for partition in partitions {
            match deploy_to_usb(
                &output,
                &partition,
                &custom_board,
                &mut storage_usb,
                ProgressBarReporter::new(),
            ) {
                Ok(_) => (),
                Err(err) => log::error!(
                    "Failed to deploy to usb with board: {:#?}\n with error: ({:?})",
                    custom_board,
                    err
                ),
            }
        }
    }

    if serial {
        use std::process;
        use std::sync::{Arc, Mutex};
        use std::time::Duration;
        use std::{io, thread};

        let mut counter = 0;

        log::info!("Looking for microcontroller serial...");

        let serial_port_info = 'find_loop: loop {
            for port in serialport::available_ports()? {
                if !serial_ports_before.contains(&port) {
                    println!("Found microcontroller serial on {}", &port.port_name);
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
                            let mut port =
                                port.lock().expect("Should be able to aquire lock for port");
                            port.write_all(b"elf2flash-term\r\n").ok();
                            port.flush().ok();
                            process::exit(0);
                        }
                    };

                    if term {
                        ctrlc::set_handler(handler.clone()).expect("Error setting Ctrl-C handler");
                    }

                    let data_terminal_ready_succeeded = {
                        let mut port = port.lock().expect("Should be able to aquire lock for port");
                        port.write_data_terminal_ready(true).is_ok()
                    };
                    if data_terminal_ready_succeeded {
                        let mut serial_buf = [0; 1024];
                        loop {
                            let read = {
                                let mut port =
                                    port.lock().expect("Should be able to aquire lock for port");
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
                                    if term {
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
