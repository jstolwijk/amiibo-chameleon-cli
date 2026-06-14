# Amiibo

The goal of this project is a Rust command-line application for macOS that
loads an Amiibo onto a Chameleon Ultra. The user provides an Amiibo name. The
application clones or updates `https://github.com/AmiiboDB/Amiibo`, resolves the
name to a validated dump, and performs all steps needed to flash and verify it.

## Status

The first implementation slice is available:

- `amiibo database update` clones or fast-forwards the local AmiiboDB cache.
- `amiibo search <name>` indexes validated 540-byte dumps and reports exact
  matches, duplicates, or close suggestions.
- `--series` disambiguates duplicate names and `--offline` prevents network
  access.
- `amiibo slots` discovers a Chameleon Ultra over macOS USB serial and displays
  firmware, active slot, tag types, and HF/LF enablement without changing the
  device.
- `amiibo backup --slot <1-8> --output <file>` captures complete NTAG emulator
  state in a versioned JSON recovery artifact and restores the previously active
  slot after reading.
- `amiibo flash <name> --slot <1-8> --force` resolves an exact database entry,
  configures NTAG215 emulation, writes pages 0 through 134, and verifies all
  memory and emulator settings before reporting success.

Standalone restore and per-device process locking are not implemented yet.

All requirements and their implementation status are tracked in the
`Requirements Tracking` section of that document. New requirements must be
added to the tracker and updated as implementation and verification progress.

## Development

```text
cargo test
cargo run -- database update
cargo run -- search "Mario" --series "Super Smash Bros Amiibo" --offline
cargo run -- slots
cargo run -- backup --slot 2 --output slot-2-backup.json
cargo run -- flash "Mario" --series "Super Smash Bros Amiibo" --slot 6 --offline --force
```
