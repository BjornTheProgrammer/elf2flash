# elf2flash

A tool for converting ELF binaries into UF2 format and flashing them to a microcontroller (RP2040, RP2350, CircuitPlaygroundBluefruit, etc.) and other supported boards.

```bash
cargo install elf2flash
```

## Options

```
Usage: elf2flash [OPTIONS] [COMMAND]

Commands:
  convert  Convert ELF to UF2 file on disk
  deploy   Deploy ELF directly to a connected board
  help     Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose <VERBOSE>  Set the logging verbosity [default: info] [possible values: off, error, warn, info, debug, trace]
  -h, --help               Print help
  -V, --version            Print version
```

### Deploying

```
Usage: elf2flash deploy [OPTIONS] <INPUT>

Arguments:
  <INPUT>  Input ELF file

Options:
  -b, --board <BOARD>
          Same options as convert…
  -v, --verbose <VERBOSE>
          Set the logging verbosity [default: info] [possible values: off, error, warn, info, debug, trace]
  -f, --family <FAMILY>
          Override family ID
  -e, --flash-sector-erase-size <FLASH_SECTOR_ERASE_SIZE>
          Flash erase sector size
  -p, --page-size <PAGE_SIZE>
          Page size
  -s, --serial
          Connect to serial after deploy
  -t, --term
          Send termination message on Ctrl+C
  -h, --help
          Print help
```

Family IDs can be referenced from [uf2families.json](https://github.com/microsoft/uf2/blob/master/utils/uf2families.json).
You can pass values in decimal (`12345`), hexadecimal (`0xe48bff59`), or binary (`0b1010...`) formats.

## Usage

To make your Rust project automatically flash the microcontroller whenever you run `cargo run`, add this to your `.cargo/config.toml`.

```toml
[target.'cfg(all(target_arch = "arm", target_os = "none"))']
runner = "elf2flash deploy -t -s"

[build]
# target = "thumbv6m-none-eabi" # Pico 1 / Cortex-M0/M0+
target = "thumbv8m.main-none-eabihf" # Pico 2 / Cortex-M23/M33

[env]
DEFMT_LOG = "debug"
```

If multiple boards are connected, `elf2flash` will detect them and attempt to flash each valid UF2 partition automatically.
You can also force a specific board using `--board rp2040` or `--board rp2350`.

## Adding support for a board

If you want to flash to an unsupported uf2 board, just add in the flags `--family`, `--flash-sector-erase-size`, and `--page-size`, these have resonable defaults, so if you are unsure what the value is, just don't provide it, and attempt running.

If you wish to add a new deafult supported board, open a PR or an issue with the board you wish to support.

If you open a PR just add a new board under `./crates/elf2flash-core/src/boards/`.

Here is an example for supporting the circuit_playground_bluefruit board.

```rust
use crate::boards::{BoardInfo, UsbDevice};

#[derive(Debug, Default, Clone)]
pub struct CircuitPlaygroundBluefruit;

impl BoardInfo for CircuitPlaygroundBluefruit {
    fn is_device_board(&self, device: &UsbDevice) -> bool {
        // https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/master/src/boards/circuitplayground_nrf52840/board.h
        if device.vendor_id != 0x239A {
            return false;
        }
        match device.product_id {
            0x0045 => true,
            _ => false,
        }
    }

    fn family_id(&self) -> u32 {
        0xada52840
    }

    fn board_name(&self) -> String {
        "circuit_playground_bluefruit".to_string()
    }
}
```

If adding the flags doesn't work, please create an issue for your board.

## Why this project instead of elf2uf2?

This project:

* Fixes several issues in [`elf2uf2-rs`](https://github.com/JoNil/elf2uf2-rs)
  ([#36](https://github.com/JoNil/elf2uf2-rs/pull/36), [#38](https://github.com/JoNil/elf2uf2-rs/issues/38), [#40](https://github.com/JoNil/elf2uf2-rs/issues/40), [#41](https://github.com/JoNil/elf2uf2-rs/pull/41), [#42](https://github.com/JoNil/elf2uf2-rs/pull/42))
* Provides a reusable library (`elf2flash-core`) for programmatic use
* Supports multiple families and explicit board selection
* Adds progress reporting, automatic board/partition detection, and optional serial logging after deploy

## Credits

Thanks to [JoNil](https://github.com/JoNil) for the excellent [`elf2uf2-rs`](https://github.com/JoNil/elf2uf2-rs) project, which this builds upon and extends.
