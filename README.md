# Tool for going from elf to flashing a pico

```bash
cargo install elf2flash
```

## Options
```
Usage: elf2flash [OPTIONS] <INPUT> [OUTPUT]

Arguments:
  <INPUT>   Input file
  [OUTPUT]  Output file

Options:
  -v, --verbose          Verbose
  -d, --deploy           Deploy to any connected pico
  -s, --serial           Connect to serial after deploy
  -t, --term             Send termination message (b"elf2flash-term\r\n") to the device on ctrl+c
  -f, --family <FAMILY>  Select family ID for UF2. See https://github.com/microsoft/uf2/blob/master/utils/uf2families.json for list
  -h, --help             Print help
  -V, --version          Print version
```

## Usage with Pico

To make your rust project flash the microcontroller whenever you run `cargo run`, add the following to your `.cargo/config.toml`. Remove the family id to flash to Pico 1.

```toml
[target.'cfg(all(target_arch = "arm", target_os = "none"))']
# runner = "elf2flash -d -t -s" # Pico 1
runner = "elf2flash -d -t -s --family 0xe48bff59" # Pico 2

[build]
# target = "thumbv6m-none-eabi" # Pico 1 / Cortex-M0 and Cortex-M0+
target = "thumbv8m.main-none-eabihf" # Pico 2 / Cortex-M23 and Cortex-M33

[env]
DEFMT_LOG = "debug"
```

In a future version family id will probably be automatically detected. In the meantime you can reference [this](https://github.com/microsoft/uf2/blob/master/utils/uf2families.json).

## Why this project instead of elf2uf2?
This project fixes some issues of elf2uf2-rs ([#36](https://github.com/JoNil/elf2uf2-rs/pull/36), [#38](https://github.com/JoNil/elf2uf2-rs/issues/38), [#40](https://github.com/JoNil/elf2uf2-rs/issues/40), [#41](https://github.com/JoNil/elf2uf2-rs/pull/41), [#42](https://github.com/JoNil/elf2uf2-rs/pull/42)), provides a library for programmatic use (`elf2flash-core`), and supports different family id's for uf2.

## Credits
A large amount of thanks for `JoNil`, amazing library [elf2uf2](https://github.com/JoNil/elf2uf2-rs), which this project largely shares codebases.
