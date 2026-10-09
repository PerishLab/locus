use crate::day;
use crate::store::{Filter, Producer, Record, Store, Window};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const CHUNK: usize = 10_000;
const SEALED: u64 = u64::MAX / 2;

pub fn migrate(legacy: &Path, store: &dyn Store) -> Result<usize, String> {
    let mut days = 0;
    for directory in listed(legacy)? {
        let name = named(&directory);
        let producer = Producer::new(&name)?;
        for file in listed(&directory)? {
            let stem = named(&file);
            let day = stem
                .strip_suffix(".jsonl")
                .and_then(day::parse)
                .ok_or_else(|| format!("{} is not a stored day", file.display()))?;
            carry(store, &producer, day, &file)?;
            fs::remove_file(&file).map_err(|error| error.to_string())?;
            days += 1;
        }
        fs::remove_dir(&directory).map_err(|error| error.to_string())?;
    }
    fs::remove_dir(legacy).map_err(|error| error.to_string())?;
    Ok(days)
}

fn carry(store: &dyn Store, producer: &Producer, day: u64, file: &Path) -> Result<(), String> {
    let bytes =
        fs::read(file).map_err(|error| format!("cannot read {}: {error}", file.display()))?;
    let mut lines: Vec<&[u8]> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    let records = lines
        .iter()
        .map(|line| Record::parse(line))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{} holds an invalid record: {error}", file.display()))?;
    for (index, chunk) in records.chunks(CHUNK).enumerate() {
        let token = format!("migrate/{producer}/{}/{index}", named(file));
        store.append(producer, chunk, &token)?;
    }
    store.tick(SEALED)?;
    let filter = Filter {
        window: Window {
            from: Some(day * day::NANOS),
            to: Some((day + 1) * day::NANOS),
        },
        selector: None,
        producer: Some(producer.clone()),
    };
    let mut carried = Vec::new();
    store.read(&filter, &mut |line| {
        carried.push(line.strip_suffix(b"\n").unwrap_or(line).to_vec());
        Ok(())
    })?;
    lines.sort_unstable();
    carried.sort_unstable();
    if carried.len() != lines.len()
        || carried
            .iter()
            .zip(&lines)
            .any(|(left, right)| left != right)
    {
        return Err(format!(
            "{} did not read back byte for byte ({} stored, {} read)",
            file.display(),
            lines.len(),
            carried.len()
        ));
    }
    Ok(())
}

fn listed(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = match fs::read_dir(directory) {
        Ok(entries) => entries
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.to_string()),
    };
    paths.sort();
    Ok(paths)
}

fn named(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}
