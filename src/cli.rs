use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use strsim::jaro_winkler;

use crate::backup::BackupArtifact;
use crate::database::{self, Entry};
use crate::device::Connection;
use crate::error::Result;
use crate::names;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Search the cached AmiiboDB by Amiibo name.
    Search(SearchArgs),
    /// Manage the local AmiiboDB checkout.
    Database {
        #[command(subcommand)]
        command: DatabaseCommand,
    },
    /// Show Chameleon Ultra slot types and enablement.
    Slots(DeviceArgs),
    /// Save a complete NTAG slot backup for later recovery.
    Backup(BackupArgs),
    /// Resolve, write, and verify an Amiibo on a Chameleon Ultra.
    Flash(FlashArgs),
}

#[derive(Debug, Args)]
struct SearchArgs {
    /// Amiibo name or "series / name".
    query: String,
    /// Restrict results to a series.
    #[arg(long)]
    series: Option<String>,
    /// Use the existing database cache without fetching.
    #[arg(long)]
    offline: bool,
}

#[derive(Debug, Args)]
struct DeviceArgs {
    /// Chameleon serial device, normally below /dev/cu.*.
    #[arg(long)]
    port: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct BackupArgs {
    /// Slot number to back up.
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=8))]
    slot: u8,
    /// New JSON backup file. Existing files are never overwritten.
    #[arg(long)]
    output: PathBuf,
    /// Chameleon serial device, normally below /dev/cu.*.
    #[arg(long)]
    port: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct FlashArgs {
    /// Amiibo name or "series / name".
    name: String,
    /// Target slot number.
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=8))]
    slot: u8,
    /// Restrict resolution to a series.
    #[arg(long)]
    series: Option<String>,
    /// Chameleon serial device, normally below /dev/cu.*.
    #[arg(long)]
    port: Option<PathBuf>,
    /// Use the existing database cache without fetching.
    #[arg(long)]
    offline: bool,
    /// Confirm that the target slot may be overwritten.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Subcommand)]
enum DatabaseCommand {
    /// Clone AmiiboDB or fast-forward the existing checkout.
    Update,
}

pub fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Database {
            command: DatabaseCommand::Update,
        } => {
            let path = database::cache_path()?;
            database::update(&path)?;
            println!("AmiiboDB is up to date at {}", path.display());
        }
        Command::Search(arguments) => search(arguments)?,
        Command::Slots(arguments) => slots(arguments)?,
        Command::Backup(arguments) => backup(arguments)?,
        Command::Flash(arguments) => flash(arguments)?,
    }
    Ok(())
}

fn flash(arguments: FlashArgs) -> Result<()> {
    if !arguments.force {
        return Err(crate::error::Error::Selection(
            "flashing is destructive; rerun with `--force` after checking the target slot".into(),
        ));
    }

    let entries = database::load(arguments.offline)?;
    let result = find_matches(&entries, &arguments.name, arguments.series.as_deref());
    if result.suggestions {
        let suggestions = result
            .entries
            .iter()
            .map(|entry| entry.full_name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(crate::error::Error::Selection(format!(
            "no exact Amiibo match; suggestions: {suggestions}"
        )));
    }
    if result.entries.len() != 1 {
        let matches = result
            .entries
            .iter()
            .map(|entry| entry.full_name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(crate::error::Error::Selection(if matches.is_empty() {
            format!("no Amiibo matched {:?}", arguments.name)
        } else {
            format!("Amiibo name is ambiguous; use `--series`: {matches}")
        }));
    }

    let entry = result.entries[0];
    let bytes = entry.dump.read()?;
    let nickname = nickname(&entry.full_name);
    let mut device = Connection::open(arguments.port.as_deref())?;
    device.flash_ntag215(arguments.slot, &bytes, entry.dump.uid, &nickname)?;
    println!(
        "Flashed and verified {} in slot {} on {}",
        entry.full_name,
        arguments.slot,
        device.path().display()
    );
    Ok(())
}

fn nickname(full_name: &str) -> String {
    let name = full_name.rsplit(" / ").next().unwrap_or(full_name);
    let mut end = name.len().min(32);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_owned()
}

fn backup(arguments: BackupArgs) -> Result<()> {
    let mut device = Connection::open(arguments.port.as_deref())?;
    let backup = device.backup_ntag(arguments.slot)?;
    let artifact = BackupArtifact::from_device(backup);
    artifact.write_new(&arguments.output)?;
    println!(
        "Backed up slot {} from {} to {}",
        arguments.slot,
        device.path().display(),
        arguments.output.display()
    );
    Ok(())
}

fn slots(arguments: DeviceArgs) -> Result<()> {
    let mut device = Connection::open(arguments.port.as_deref())?;
    let info = device.inspect()?;
    println!(
        "Chameleon at {}  firmware {}.{} ({})",
        device.path().display(),
        info.firmware_major,
        info.firmware_minor,
        info.git_version
    );
    if info.firmware_minor < 1 {
        println!("warning: firmware predates v2.1.0 NTAG emulation support");
    }

    for (index, slot) in info.slots.iter().enumerate() {
        let active = if info.active_slot == index as u8 + 1 {
            " active"
        } else {
            ""
        };
        println!(
            "Slot {}{}: HF {} [{}], LF {} [{}]",
            index + 1,
            active,
            slot.hf_type,
            enabled_label(slot.hf_enabled),
            slot.lf_type,
            enabled_label(slot.lf_enabled)
        );
    }
    Ok(())
}

fn enabled_label(enabled: bool) -> &'static str {
    if enabled { "enabled" } else { "disabled" }
}

fn search(arguments: SearchArgs) -> Result<()> {
    let entries = database::load(arguments.offline)?;
    let result = find_matches(&entries, &arguments.query, arguments.series.as_deref());

    if result.entries.is_empty() {
        println!("No Amiibo matched {:?}.", arguments.query);
        return Ok(());
    }

    if result.suggestions {
        println!("No exact match. Suggestions:");
    }

    for entry in result.entries {
        let uid = entry
            .dump
            .uid
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>();
        let warning = if entry.dump.manufacturer_warning {
            " (warning: non-NXP manufacturer byte)"
        } else {
            ""
        };
        println!("{}  UID {}{}", entry.full_name, uid, warning);
    }
    Ok(())
}

#[derive(Debug)]
struct MatchResult<'a> {
    entries: Vec<&'a Entry>,
    suggestions: bool,
}

fn find_matches<'a>(entries: &'a [Entry], query: &str, series: Option<&str>) -> MatchResult<'a> {
    let normalized_query = names::normalize(query);
    let normalized_series = series.map(names::normalize);

    let filtered: Vec<_> = entries
        .iter()
        .filter(|entry| {
            normalized_series
                .as_ref()
                .is_none_or(|series| entry.normalized_series == *series)
        })
        .collect();

    let exact_full: Vec<_> = filtered
        .iter()
        .copied()
        .filter(|entry| entry.normalized_full_name == normalized_query)
        .collect();
    if !exact_full.is_empty() {
        return MatchResult {
            entries: exact_full,
            suggestions: false,
        };
    }

    let exact_short: Vec<_> = filtered
        .iter()
        .copied()
        .filter(|entry| entry.normalized_name == normalized_query)
        .collect();
    if !exact_short.is_empty() {
        return MatchResult {
            entries: exact_short,
            suggestions: false,
        };
    }

    let mut suggestions: Vec<_> = filtered
        .into_iter()
        .map(|entry| {
            let score = jaro_winkler(&normalized_query, &entry.normalized_name)
                .max(jaro_winkler(&normalized_query, &entry.normalized_full_name));
            (score, entry)
        })
        .filter(|(score, entry)| {
            *score >= 0.72
                || entry.normalized_name.contains(&normalized_query)
                || entry.normalized_full_name.contains(&normalized_query)
        })
        .collect();
    suggestions.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.full_name.cmp(&right.1.full_name))
    });

    MatchResult {
        entries: suggestions
            .into_iter()
            .map(|(_, entry)| entry)
            .take(10)
            .collect(),
        suggestions: true,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::find_matches;
    use crate::database::Entry;
    use crate::dump::ValidatedDump;
    use crate::names;

    fn entry(series: &str, name: &str) -> Entry {
        let full_name = format!("{series} / {name}");
        Entry {
            normalized_series: names::normalize(series),
            normalized_name: names::normalize(name),
            normalized_full_name: names::normalize(&full_name),
            full_name,
            dump: ValidatedDump {
                path: PathBuf::from("fixture.bin"),
                uid: [0; 7],
                manufacturer_warning: false,
            },
        }
    }

    #[test]
    fn returns_all_duplicate_short_names() {
        let entries = vec![entry("Series A", "Mario"), entry("Series B", "Mario")];
        let result = find_matches(&entries, "mario", None);
        assert_eq!(result.entries.len(), 2);
        assert!(!result.suggestions);
    }

    #[test]
    fn series_disambiguates_short_name() {
        let entries = vec![entry("Series A", "Mario"), entry("Series B", "Mario")];
        let result = find_matches(&entries, "mario", Some("series b"));
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].full_name, "Series B / Mario");
    }

    #[test]
    fn returns_ranked_suggestions_for_a_typo() {
        let entries = vec![entry("Series A", "Mario"), entry("Series A", "Link")];
        let result = find_matches(&entries, "Mairo", None);
        assert!(result.suggestions);
        assert_eq!(result.entries[0].full_name, "Series A / Mario");
    }
}
