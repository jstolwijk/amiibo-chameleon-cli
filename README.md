# amiibo

`amiibo` is a macOS command-line application that finds Amiibo dumps in
[AmiiboDB](https://github.com/AmiiboDB/Amiibo) and loads them onto a
[Chameleon Ultra](https://github.com/RfidResearchGroup/ChameleonUltra).

The application can search the database, inspect the Chameleon's eight slots,
create and restore NTAG backups, and flash a validated Amiibo dump to an
NTAG215 slot.

## Requirements

- macOS
- Rust and Cargo
- Git
- A Chameleon Ultra connected over USB
- Chameleon Ultra firmware major version 2; firmware v2.1.0 or newer is
  recommended for NTAG emulation
- Network access when initially downloading or updating AmiiboDB

Use Amiibo data only where you have the legal right to do so. AmiiboDB is an
external project, and its dumps are not maintained or guaranteed by this
application.

## Installation

Build a release executable:

```sh
cargo build --release
```

The executable is created at:

```text
target/release/amiibo
```

Run it directly, install it with Cargo, or use `cargo run --` while developing:

```sh
cargo install --path .
amiibo --help
```

```sh
cargo run -- --help
```

All examples below use the installed `amiibo` command. When using Cargo,
replace `amiibo` with `cargo run --`.

## Quick Start

1. Download or update AmiiboDB:

   ```sh
   amiibo database update
   ```

2. Find the exact Amiibo name and series:

   ```sh
   amiibo search "Mario"
   ```

3. Connect the Chameleon Ultra and inspect its slots:

   ```sh
   amiibo slots
   ```

4. Back up an existing NTAG slot before replacing it:

   ```sh
   amiibo backup --slot 3 --output slot-3-backup.json
   ```

5. Flash the selected Amiibo:

   ```sh
   amiibo flash "Mario" \
     --series "Super Smash Bros Amiibo" \
     --slot 3 \
     --force
   ```

Flashing and restoring modify the selected slot and therefore require
`--force`.

## Database Commands

### Update AmiiboDB

```text
amiibo database update
```

Clones AmiiboDB into the local cache on first use. On later runs it fetches
changes and fast-forwards the existing checkout.

The default cache location is:

```text
~/Library/Caches/amiibo/AmiiboDB
```

Set `AMIIBO_DATABASE_PATH` to use another checkout:

```sh
AMIIBO_DATABASE_PATH=/path/to/AmiiboDB amiibo search "Link" --offline
```

The custom directory must be a Git checkout containing an `Amiibo Bin`
directory.

### Search

```text
amiibo search [OPTIONS] <QUERY>
```

Options:

| Option | Description |
| --- | --- |
| `--series <SERIES>` | Restrict results to one series. |
| `--offline` | Use the cached database without fetching updates. |

Examples:

```sh
amiibo search "Mario"
amiibo search "Mario" --series "Super Smash Bros Amiibo"
amiibo search "Super Smash Bros Amiibo / Mario" --offline
```

Searches are insensitive to case and common punctuation or filename
separators. An exact short name can return multiple entries from different
series. Misspelled or partial names may return up to ten suggestions.

Without `--offline`, the command updates AmiiboDB before searching. With
`--offline`, the cache must already exist.

## Device Commands

The application automatically searches common macOS serial devices such as
`/dev/cu.usbmodem*`, `/dev/cu.usbserial*`, and `/dev/cu.wchusbserial*`.

Use `--port` when more than one device is connected or automatic discovery
does not select the correct device:

```sh
amiibo slots --port /dev/cu.usbmodem123456
```

Only one `amiibo` process can use a serial device at a time.

### Inspect Slots

```text
amiibo slots [--port <PATH>]
```

Displays the connected device, firmware version, active slot, HF and LF tag
types, and whether each side of every slot is enabled.

```sh
amiibo slots
amiibo slots --port /dev/cu.usbmodem123456
```

This command does not modify the device.

### Back Up a Slot

```text
amiibo backup --slot <1-8> --output <FILE> [--port <PATH>]
```

Creates a versioned JSON backup containing the complete NTAG/MIFARE Ultralight
emulator state for the selected slot.

```sh
amiibo backup --slot 2 --output slot-2-backup.json
```

The slot must currently contain an NTAG or MIFARE Ultralight HF tag type. The
output file must not already exist; existing files are never overwritten.
After reading the backup, the command restores the slot that was active before
the operation.

### Restore a Backup

```text
amiibo restore <BACKUP> --slot <1-8> [--port <PATH>] --force
```

Restores a JSON backup to the selected target slot and verifies the restored
state by reading it back.

```sh
amiibo restore slot-2-backup.json --slot 2 --force
```

The target slot can differ from the slot recorded in the backup. Restore is not
transactional, so do not disconnect or reset the device while it is running.

### Flash an Amiibo

```text
amiibo flash [OPTIONS] <NAME> --slot <1-8> --force
```

Options:

| Option | Description |
| --- | --- |
| `--slot <1-8>` | Select the target Chameleon slot. |
| `--series <SERIES>` | Restrict name resolution to one series. |
| `--port <PATH>` | Use a specific serial device. |
| `--offline` | Use the cached database without fetching updates. |
| `--force` | Confirm that the target slot may be overwritten. |

Examples:

```sh
amiibo flash "Zelda" --slot 4 --force
```

```sh
amiibo flash "Mario" \
  --series "Super Smash Bros Amiibo" \
  --slot 6 \
  --offline \
  --force
```

The name must resolve to exactly one database entry. If it is ambiguous, use
`--series` or the full `series / name` value shown by `amiibo search`.
Suggestions are never flashed automatically.

Before modifying the slot, the application validates the 540-byte NTAG215 dump.
It then configures NTAG215 emulation, writes all pages and Amiibo
authentication values, enables HF emulation, assigns a nickname, and verifies
the complete result by reading it back.

If the target already contains an NTAG-family HF tag, the application creates a
temporary recovery backup and attempts to roll it back if flashing fails. A
successful flash removes the temporary backup. If recovery also fails, the
error reports the retained recovery file under:

```text
~/Library/Caches/amiibo/recovery/
```

For safety, flashing refuses to overwrite a slot containing a non-NTAG HF tag.

## Offline Use

After running `amiibo database update` at least once, database-dependent
commands can avoid all network access:

```sh
amiibo search "Samus" --offline
amiibo flash "Samus" --slot 5 --offline --force
```

## Help and Version

```sh
amiibo --help
amiibo <COMMAND> --help
amiibo --version
```

## Development

Run the test suite:

```sh
cargo test
```

The application currently supports macOS only.
