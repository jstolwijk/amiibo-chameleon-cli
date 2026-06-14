use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::dump::{self, ValidatedDump};
use crate::error::{Error, Result};
use crate::names;

const DATABASE_URL: &str = "https://github.com/AmiiboDB/Amiibo.git";

#[derive(Debug, Clone)]
pub struct Entry {
    pub full_name: String,
    pub normalized_series: String,
    pub normalized_name: String,
    pub normalized_full_name: String,
    pub dump: ValidatedDump,
}

pub fn cache_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("AMIIBO_DATABASE_PATH") {
        return Ok(PathBuf::from(path));
    }

    let home = env::var_os("HOME").ok_or(Error::CacheUnavailable)?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Caches")
        .join("amiibo")
        .join("AmiiboDB"))
}

pub fn update(path: &Path) -> Result<()> {
    if path.exists() {
        ensure_checkout(path)?;
        run_git(path, &["fetch", "--prune", "origin"])?;
        run_git(path, &["pull", "--ff-only"])?;
    } else {
        let parent = path.parent().ok_or(Error::CacheUnavailable)?;
        fs::create_dir_all(parent)?;
        let output = Command::new("git")
            .args(["clone", "--depth", "1", DATABASE_URL])
            .arg(path)
            .output()?;
        ensure_git_success(output)?;
    }

    ensure_checkout(path)
}

pub fn load(offline: bool) -> Result<Vec<Entry>> {
    let path = cache_path()?;
    if offline {
        if !path.exists() {
            return Err(Error::DatabaseMissing(path));
        }
    } else {
        update(&path)?;
    }
    index(&path)
}

pub fn index(repository: &Path) -> Result<Vec<Entry>> {
    ensure_checkout(repository)?;
    let bin_root = repository.join("Amiibo Bin");
    let mut entries = Vec::new();
    visit_directory(&bin_root, &bin_root, &mut entries)?;
    entries.sort_by(|left, right| left.full_name.cmp(&right.full_name));
    Ok(entries)
}

fn visit_directory(directory: &Path, bin_root: &Path, entries: &mut Vec<Entry>) -> Result<()> {
    for item in fs::read_dir(directory)? {
        let item = item?;
        let path = item.path();
        let file_name = item.file_name();

        if is_hidden(&file_name) || file_name == OsStr::new("!Essential Files") {
            continue;
        }

        let file_type = item.file_type()?;
        if file_type.is_dir() {
            visit_directory(&path, bin_root, entries)?;
        } else if file_type.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bin"))
            && item.metadata()?.len() == dump::DUMP_SIZE as u64
        {
            if let Ok(validated) = dump::validate(&path, bin_root) {
                entries.push(entry_from_path(&path, bin_root, validated)?);
            }
        }
    }
    Ok(())
}

fn entry_from_path(path: &Path, bin_root: &Path, dump: ValidatedDump) -> Result<Entry> {
    let relative = path
        .strip_prefix(bin_root)
        .map_err(|_| Error::InvalidDump {
            path: path.to_owned(),
            reason: "path is not below Amiibo Bin".into(),
        })?;
    let series = relative
        .parent()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .unwrap_or("Uncategorized")
        .to_owned();
    let name = path
        .file_stem()
        .and_then(OsStr::to_str)
        .ok_or_else(|| Error::InvalidDump {
            path: path.to_owned(),
            reason: "filename is not valid UTF-8".into(),
        })?
        .to_owned();
    let full_name = format!("{series} / {name}");

    Ok(Entry {
        normalized_series: names::normalize(&series),
        normalized_name: names::normalize(&name),
        normalized_full_name: names::normalize(&full_name),
        full_name,
        dump,
    })
}

fn ensure_checkout(path: &Path) -> Result<()> {
    if path.join(".git").is_dir() && path.join("Amiibo Bin").is_dir() {
        Ok(())
    } else {
        Err(Error::InvalidDatabase(path.to_owned()))
    }
}

fn run_git(repository: &Path, arguments: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()?;
    ensure_git_success(output)
}

fn ensure_git_success(output: std::process::Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(Error::GitFailed(if message.is_empty() {
        format!("process exited with {}", output.status)
    } else {
        message
    }))
}

fn is_hidden(name: &OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::index;
    use crate::dump::DUMP_SIZE;

    fn temp_repository() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("amiibo-test-{unique}"));
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("Amiibo Bin").join("Series A")).unwrap();
        fs::create_dir_all(root.join("Amiibo Bin").join("Series B")).unwrap();
        fs::create_dir_all(root.join("Amiibo Bin").join("!Essential Files")).unwrap();
        root
    }

    fn write_valid_dump(path: &std::path::Path) {
        let mut bytes = vec![0; DUMP_SIZE];
        bytes[0..3].copy_from_slice(&[0x04, 1, 2]);
        bytes[3] = 0x88 ^ bytes[0] ^ bytes[1] ^ bytes[2];
        bytes[4..8].copy_from_slice(&[3, 4, 5, 6]);
        bytes[8] = bytes[4] ^ bytes[5] ^ bytes[6] ^ bytes[7];
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn indexes_valid_dumps_and_excludes_support_files() {
        let repository = temp_repository();
        write_valid_dump(&repository.join("Amiibo Bin/Series A/Mario.bin"));
        write_valid_dump(&repository.join("Amiibo Bin/Series B/Mario.bin"));
        write_valid_dump(&repository.join("Amiibo Bin/!Essential Files/key.bin"));
        fs::write(
            repository.join("Amiibo Bin/Series A/invalid.bin"),
            vec![0; 12],
        )
        .unwrap();

        let entries = index(&repository).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].full_name, "Series A / Mario");
        assert_eq!(entries[1].full_name, "Series B / Mario");

        fs::remove_dir_all(repository).unwrap();
    }
}
