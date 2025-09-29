use elf2flash_core::{
    ProgressReporter,
    boards::{BoardIter, UsbDevice, UsbVersion},
    elf2uf2,
};
use env_logger::Env;
use pbr::{ProgressBar, Units};
use std::{
    error::Error,
    fs::{self, File},
    io::{BufReader, BufWriter, Stdout, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
};
use sysinfo::Disks;

use clap::Parser;

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
    #[cfg(feature = "serial")]
    #[clap(short, long)]
    serial: bool,

    /// Send termination message (b"elf2flash-term\r\n") to the device on ctrl+c
    #[cfg(feature = "serial")]
    #[clap(short, long)]
    term: bool,

    /// Select family ID for UF2. See https://github.com/microsoft/uf2/blob/master/utils/uf2families.json for list.
    #[clap(short, long, value_parser = num_parser)]
    family: Option<u32>,

    /// Input file
    input: String,

    /// Output file
    output: Option<String>,
}

// allow user to pass hex formatted numbers (typically the format used by family ids)
fn num_parser(s: &str) -> Result<u32, &'static str> {
    match s.get(0..2) {
        Some("0x") => u32::from_str_radix(&s[2..], 16).map_err(|_| "invalid hex number"),
        Some("0b") => u32::from_str_radix(&s[2..], 2).map_err(|_| "invalid binary number"),
        _ => s.parse::<u32>().map_err(|_| "invalid decimal number"),
    }
}

impl Opts {
    fn output_path(&self) -> PathBuf {
        if let Some(output) = &self.output {
            Path::new(output).with_extension("uf2")
        } else {
            Path::new(&self.input).with_extension("uf2")
        }
    }

    fn global() -> &'static Opts {
        OPTS.get().expect("Opts is not initialized")
    }
}

static OPTS: OnceLock<Opts> = OnceLock::new();

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
    OPTS.set(Opts::parse()).unwrap();

    if Opts::global().verbose {
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
    }

    #[cfg(feature = "usb")]
    {
        log::debug!("Searching for devices via usb...");
        let devices = rusb::devices().unwrap();
        let devices = devices
            .iter()
            .map(|device| {
                let desc = device.device_descriptor().unwrap();
                let version = desc.device_version();
                UsbDevice {
                    bus_number: device.bus_number(),
                    address: device.address(),
                    vendor_id: desc.vendor_id(),
                    product_id: desc.product_id(),
                    version: UsbVersion(version.0, version.1, version.2),
                }
            })
            .collect::<Vec<_>>();

        let boards = Vec::new();

        for device in devices {
            let board = match BoardIter::new().find(|board| board.is_device_board(&device)) {
                Some(board) => board,
                None => continue,
            };

            boards.push((device, board));
        }
    }

    #[cfg(feature = "serial")]
    let serial_ports_before = serialport::available_ports()?;

    let mut deployed_path = None;
    let input = BufReader::new(File::open(&Opts::global().input)?);

    let output = if Opts::global().deploy {
        let disks = Disks::new_with_refreshed_list();

        let mut pico_drive = None;
        for disk in &disks {
            let mount = disk.mount_point();

            if mount.join("INFO_UF2.TXT").is_file() {
                println!("Found pico uf2 disk {}", &mount.to_string_lossy());
                pico_drive = Some(mount.to_owned());
                break;
            }
        }

        if let Some(pico_drive) = pico_drive {
            deployed_path = Some(pico_drive.join("out.uf2"));
            File::create(deployed_path.as_ref().unwrap())?
        } else {
            return Err("Unable to find mounted pico".into());
        }
    } else {
        File::create(Opts::global().output_path())?
    };

    let family_id = Opts::global().family;
    if let Some(family_id) = family_id {
        println!("Using UF2 Family ID 0x{:x}", family_id);
    }

    if let Err(err) = elf2uf2(
        input,
        BufWriter::new(output),
        family_id,
        ProgressBarReporter::new(),
    ) {
        if Opts::global().deploy {
            fs::remove_file(deployed_path.unwrap())?;
        } else {
            fs::remove_file(Opts::global().output_path())?;
        }
        return Err(err);
    }

    // New line after progress bar
    println!();

    #[cfg(feature = "serial")]
    if Opts::global().serial {
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

                    if Opts::global().term {
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
                                    if Opts::global().term {
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
