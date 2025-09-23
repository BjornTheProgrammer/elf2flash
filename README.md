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
  -t, --term             Send termination message to the device on ctrl+c
  -f, --family <FAMILY>  Select family ID for UF2. See https://github.com/microsoft/uf2/blob/master/utils/uf2families.json for list
  -h, --help             Print help
  -V, --version          Print version
```

## Why this project instead of elf2uf2?
This project fixes some issues of elf2uf2-rs ([#36](https://github.com/JoNil/elf2uf2-rs/pull/36), [#38](https://github.com/JoNil/elf2uf2-rs/issues/38), [#40](https://github.com/JoNil/elf2uf2-rs/issues/40), [#41](https://github.com/JoNil/elf2uf2-rs/pull/41), [#42](https://github.com/JoNil/elf2uf2-rs/pull/42)), provides a library for programmatic use (`elf2flash-core`), and .

## Credits
A large amount of thanks for `JoNil`, amazing library [elf2uf2](https://github.com/JoNil/elf2uf2-rs), which this project largely shares codebases.
