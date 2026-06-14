# Amiibo to Chameleon Ultra: Proposed Workflow

Research date: 2026-06-14

## Scope

The application will be a Rust command-line program for macOS. The user supplies
an Amiibo name, not a file. The application resolves that name to a validated
`.bin` dump from a local clone of the
[AmiiboDB/Amiibo](https://github.com/AmiiboDB/Amiibo) repository and loads it
into one of the Chameleon Ultra's eight emulation slots over USB.

The application will clone the repository on first use and update the existing
clone on later uses. AmiiboDB is an external data source and currently has no
explicit license in its repository. Its README also states that not every dump
has been tested and identifies some known problematic entries. These facts must
be visible in project documentation and must not be represented as a guarantee
that every database entry works.

## Findings

- Amiibo use an NTAG215-compatible NFC memory layout.
- A normal raw Amiibo dump is 540 bytes: 135 pages of four bytes each.
- Chameleon Ultra firmware v2.1.0 supports NTAG215 emulation.
- The Chameleon firmware reports 135 writable NTAG215 memory pages through the
  page API. Emulator counter state is stored separately and accessed through
  dedicated counter commands. A normal 540-byte dump therefore exactly fills
  pages 0 through 134.
- Loading memory alone is insufficient. The Chameleon's ISO14443-A
  anti-collision UID is stored separately and must match the UID encoded in
  pages 0 through 2 of the dump.
- The Chameleon does not decrypt, encrypt, or sign Amiibo application data. The
  supplied dump must already be valid encrypted Amiibo data.
- The current official CLI can perform the complete operation. For the actual
  application, using the device protocol directly is preferable to scripting
  an interactive Python process.
- AmiiboDB contains duplicate display names across series. For example, `Mario`
  is present in multiple directories, so name lookup must support
  disambiguation.
- AmiiboDB also contains key files and other non-tag artifacts under
  `Amiibo Bin/!Essential Files`. The application must never treat these as
  selectable Amiibo.

## Platform And Language

- Implementation language: Rust.
- Supported platform: macOS only.
- Distribution target: a native command-line executable.
- Device transport: macOS USB serial devices, normally exposed below
  `/dev/cu.*`.
- Repository cache: the macOS user cache directory, for example
  `~/Library/Caches/amiibo/AmiiboDB`.

Linux and Windows support are explicitly outside the first version.

## Amiibo Database

Database URL:

```text
https://github.com/AmiiboDB/Amiibo.git
```

The database workflow is:

1. On first use, clone the repository into the macOS user cache directory.
2. On later uses, fetch and fast-forward the cached clone before resolving a
   name. Provide an explicit offline option to use the existing clone without
   network access.
3. Index only files below `Amiibo Bin/` whose extension is `.bin` and whose size
   is exactly 540 bytes.
4. Exclude `Amiibo Bin/!Essential Files/` and all hidden or support files.
5. Build each selectable identity from both its series directory and filename,
   for example `Super Smash Bros Amiibo / Mario`.
6. Normalize user input for case, whitespace, punctuation, Unicode composition,
   and common filename separators.
7. Prefer an exact normalized full-name match.
8. If a short name has one match, select it.
9. If a short name has multiple matches, print the candidates and require the
   user to select one. Never silently choose among duplicates.
10. If there is no exact match, show close suggestions but do not flash until
    one candidate is explicitly selected.

The current repository snapshot contains files of several sizes. File extension
alone is therefore insufficient validation.

## Dump Validation

After resolving a name and before modifying the device:

1. Confirm the selected file remains inside the cached repository's
   `Amiibo Bin/` directory after canonicalizing the path.
2. Confirm the file is exactly 540 bytes.
3. Extract the seven-byte UID:

   ```text
   UID = dump[0..3] + dump[4..8]
   ```

   In index notation this is bytes `0,1,2,4,5,6,7`.

4. Validate the NTAG UID check bytes:

   ```text
   dump[3] == 0x88 XOR dump[0] XOR dump[1] XOR dump[2]
   dump[8] == dump[4] XOR dump[5] XOR dump[6] XOR dump[7]
   ```

5. Require a seven-byte UID. Warn, rather than initially reject, if the
   manufacturer byte is not `0x04`.
6. Validate the plaintext NTAG215 dynamic-lock, configuration, PWD, and PACK
   page positions without modifying them. The capability container is within
   the encrypted Amiibo payload and is not directly inspectable.

Structural checks cannot prove that a dump is cryptographically valid. Without
cryptographic validation, the command should describe the input as
"NTAG215-shaped", not "verified Amiibo". The application must not read or use
the retail key files included in AmiiboDB.

## Device Operation

The proposed high-level operation is:

1. Connect over USB serial.
2. Read firmware and Git versions.
3. Refuse incompatible firmware major versions. Warn when firmware predates
   v2.1.0, where proper MFU/NTAG emulation was added.
4. Read all slot metadata and validate the requested slot.
5. Back up the target slot's type, enable state, anti-collision configuration,
   emulator pages, version data, signature data, write mode, and nickname.
6. Set the target slot type to `NTAG_215`. This also initializes its data.
7. Select the target slot because the MFU page commands operate on the active
   slot.
8. Derive the Amiibo password from the seven-byte UID and replace the
   unreadable zero placeholders in pages 133 and 134 with that password and
   PACK `80 80`.
9. Write the resulting 540-byte emulator image as pages 0 through 134.
10. Set ISO14443-A anti-collision data:

   ```text
   UID  = UID extracted from the dump
   ATQA = 44 00
   SAK  = 00
   ATS  = absent
   ```

11. Disable UID-magic mode.
12. Set write mode to `NORMAL` so games that store data can update the Amiibo.
13. Enable HF emulation for the slot.
14. Assign a nickname derived from the resolved Amiibo name.
15. Read back all 135 pages and compare them byte-for-byte with the expected
    emulator image.
16. Read back UID, ATQA, SAK, ATS, slot type, write mode, and enable state.
17. Leave the flashed slot active only after every verification succeeds.

The command must not alter UID, lock bytes, configuration pages, or Amiibo
payload data. Physical NTAG215 tags do not expose PWD/PACK, so raw dumps contain
zero placeholders in those pages. The command must replace only those
placeholders using the standard UID-derived Amiibo password and PACK `80 80`.

## Equivalent Official CLI Sequence

This sequence demonstrates the underlying operations with the current official
CLI. It is reference material, not the intended implementation:

```text
hw connect
hw slot list
hw slot type -s 1 -t NTAG_215
hw slot change -s 1
hf mfu eload -f amiibo.bin -t bin
hf mfu econfig -s 1 --uid <UID_FROM_DUMP> --atqa 4400 --sak 00 --delete-ats --disable-uid-magic --write NORMAL
hw slot enable -s 1 --hf
hf mfu eview
hf mfu econfig -s 1
hw slot list
```

`hw slot type` already initializes the slot, so a separate `hw slot init` is
not required.

## Failure Handling

- Perform all file validation before connecting or changing a slot.
- Keep a temporary backup until final verification passes.
- If a write or verification fails, attempt to restore the previous slot state.
- If restoration also fails, report both failures and retain the backup on
  disk for manual recovery.
- Refuse destructive changes when the captured counter has an active tearing
  event because the firmware protocol can clear that flag but cannot recreate
  it during rollback.
- Never overwrite an occupied slot without `--force` or an interactive
  confirmation. Non-interactive use must require `--force`.
- Use a per-device lock so two processes cannot program the same Chameleon
  concurrently.

The device protocol is not transactional, so rollback is best-effort. The CLI
must state this before the first destructive operation.

## Proposed CLI

```text
amiibo flash <name> --slot <1-8> [--series <name>] [--port <path>] [--force] [--offline]
amiibo search <query> [--series <name>] [--offline]
amiibo database update
amiibo slots [--port <path>]
amiibo backup --slot <1-8> --output <file> [--port <path>]
amiibo restore <backup> --slot <1-8> [--port <path>] [--force]
```

Example:

```text
amiibo flash "Mario" --series "Super Smash Bros Amiibo" --slot 3
```

The initial implementation can start with `search`, `database update`, `slots`,
and `flash`. Backup and rollback should still exist internally before `flash`
is considered complete.

## Rust Implementation Direction

Use a Cargo workspace or single crate with clear modules for:

- CLI parsing.
- AmiiboDB clone, update, indexing, and name resolution.
- Amiibo dump parsing and validation.
- Chameleon protocol framing.
- macOS serial-device discovery and transport.
- Flash orchestration, backup, verification, and rollback.

Use the documented Chameleon binary protocol over USB serial:

- Frame start: `0x11 0xEF`
- Big-endian command, status, and payload-length fields
- LRC checks on header and payload
- Request/response timeouts and strict response validation

Advantages over wrapping the official CLI:

- Deterministic, non-interactive behavior.
- No dependency on the official CLI's prompt or human-readable output.
- Precise rollback and verification.
- Easier unit tests using recorded protocol frames and a fake serial transport.

The protocol version must be isolated behind a device client interface so
future firmware changes do not leak into Amiibo parsing or command handling.

## Test Plan

- Unit tests for name normalization, duplicate-name handling, repository path
  containment, size checks, UID extraction, BCC checks, and malformed files.
- Fixture repository tests that include valid dumps, duplicate names, support
  files, Unicode names, and invalid file sizes.
- Database clone and update tests using a local Git remote.
- Golden tests for 540-byte input to 135 four-byte page writes.
- Protocol frame encode/decode and checksum tests.
- Fake-device tests for success, timeout, partial write, bad read-back, and
  rollback failure.
- Hardware integration test on macOS against an unused slot.
- Console smoke test with a resolved AmiiboDB entry after byte-for-byte
  verification succeeds.

## Requirements Tracking

Every product, technical, safety, and test requirement must be recorded in this
section when it is introduced. Each requirement has one of these statuses:

- `Not started`
- `In progress`
- `Implemented`
- `Verified`
- `Blocked`

A requirement may only be marked `Implemented` when the code path exists. It may
only be marked `Verified` when an automated test or documented hardware check
has passed. Pull requests and implementation summaries must list requirement
IDs affected by the change.

| ID | Requirement | Status |
| --- | --- | --- |
| R001 | Implement the application in Rust. | Verified |
| R002 | Support macOS only in v1. | Implemented |
| R003 | Accept an Amiibo name instead of a user-provided dump path. | Verified |
| R004 | Clone AmiiboDB on first use into the macOS user cache directory. | Implemented |
| R005 | Update the cached repository on later online uses. | Implemented |
| R006 | Support explicit offline use of an existing cache. | Implemented |
| R007 | Index only 540-byte `.bin` files below `Amiibo Bin/`. | Verified |
| R008 | Exclude `!Essential Files`, keys, hidden files, and support artifacts. | Verified |
| R009 | Normalize names and require explicit disambiguation of duplicates. | Verified |
| R010 | Show suggestions for unmatched names without selecting automatically. | Verified |
| R011 | Validate repository path containment, dump size, UID, BCC, and NTAG215 structure before device changes. | Verified |
| R012 | Discover and connect to a Chameleon Ultra over macOS USB serial. | Verified |
| R013 | Check firmware compatibility before flashing. | Verified |
| R014 | Back up the target slot before destructive changes. | Verified |
| R015 | Configure the slot as NTAG215 and write pages 0 through 134. | Verified |
| R016 | Configure UID, ATQA `4400`, SAK `00`, no ATS, normal writes, and HF enablement. | Verified |
| R017 | Verify all written pages and emulator settings by reading them back. | Verified |
| R018 | Attempt rollback after write or verification failure and preserve recovery data if rollback fails. | Verified |
| R019 | Prevent concurrent programming of the same device. | Verified |
| R020 | Require confirmation or `--force` before overwriting an occupied slot. | Verified |
| R021 | Never read or use retail key files from AmiiboDB. | Verified |
| R022 | Expose `search`, `database update`, `slots`, and `flash` commands. | Verified |
| R023 | Cover database resolution, dump validation, protocol, rollback, and hardware behavior with the test plan above. | Verified |
| R024 | Document AmiiboDB's missing explicit license and upstream data-quality limitations. | Implemented |
| R025 | Maintain this requirements register and report implementation and verification status by requirement ID. | Implemented |
| R026 | Validate and restore a versioned backup artifact to a selected slot with destructive-operation confirmation and read-back verification. | Verified |

## Decisions

1. Successful flashing leaves the verified target slot active.
2. Automatic recovery artifacts are deleted after successful flashing or
   successful rollback. They are retained after rollback failure or process
   interruption.
3. Every online database lookup updates the cached repository. `--offline`
   is the explicit opt-out.

## Hardware Verification Log

- 2026-06-14: `amiibo slots` successfully discovered
  `/dev/cu.usbmodem0000000000001`, read firmware `2.1` / Git version `v2.1.0`,
  identified the active slot, and decoded all eight slot type and enablement
  records. User-reported console output reviewed.
- 2026-06-14: `amiibo backup` successfully created and exposed a complete NTAG
  slot backup artifact on the connected Chameleon Ultra. User reported the
  command and resulting artifact looked correct.
- 2026-06-14: `amiibo flash "Mario" --series "Super Smash Bros Amiibo"
  --slot 6 --offline --force` successfully resolved the dump, programmed
  NTAG215 pages 0 through 134, configured emulator settings, and passed
  byte-for-byte and configuration read-back on firmware `v2.1.0`.
- 2026-06-14: Nintendo Switch rejected that first image because the raw dump's
  unreadable PWD/PACK placeholders were written as zeroes. The implementation
  was corrected to use the official Chameleon Ultra `--amiibo` derivation from
  commit `193f66a`: UID-derived PWD and PACK `80 80`. Retail keys remain unused.
- 2026-06-14: After applying UID-derived PWD/PACK, Nintendo Switch successfully
  recognized and used the Super Smash Bros Mario Amiibo emulated from slot 6.
  This completes the end-to-end console smoke test for name resolution,
  flashing, protocol verification, and Nintendo reader compatibility.

## Primary Sources

- Chameleon Ultra repository:
  https://github.com/RfidResearchGroup/ChameleonUltra
- Official CLI source, including `hf mfu eload`, `hf mfu econfig`, and slot
  commands:
  https://github.com/RfidResearchGroup/ChameleonUltra/blob/main/software/script/chameleon_cli_unit.py
- Official protocol documentation:
  https://github.com/RfidResearchGroup/ChameleonUltraDocs/blob/main/protocol.md
- Official CLI documentation:
  https://github.com/RfidResearchGroup/ChameleonUltraDocs/blob/main/cli.md
- Firmware NTAG215 implementation:
  https://github.com/RfidResearchGroup/ChameleonUltra/blob/main/firmware/application/src/rfid/nfctag/hf/nfc_mf0_ntag.c
- `amiitool`, for Amiibo encryption, signing, and optional key-based
  verification:
  https://github.com/socram8888/amiitool
- AmiiboDB data repository:
  https://github.com/AmiiboDB/Amiibo
