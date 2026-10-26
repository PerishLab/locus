use super::*;
use std::collections::BTreeMap;
use std::path::Path;

#[test]
fn history() {
    let opened = Opened::new("adoption");
    let duplicate = format!(" {} ", atom(BASE, 0));
    let sealed = vec![duplicate.clone(), duplicate, atom(BASE + 1, 1)];
    let open = vec![atom(BASE + 2, 2), atom(BASE + 3, 3)];
    put(&opened.columns, "concord", &sealed, "one");
    opened.columns.tick(FAR).expect("seal");
    put(&opened.columns, "concord", &open, "two");
    let mut expected: Vec<Vec<u8>> = sealed
        .iter()
        .chain(&open)
        .map(|line| format!("{line}\n").into_bytes())
        .collect();
    expected.sort();
    assert_eq!(records(&opened.columns, &Filter::default()), expected);
    assert_eq!(expected.len(), 5);
    let Opened {
        _runtime: runtime,
        home,
        columns,
    } = opened;
    drop(columns);
    let binding = home.join("column/model.json");
    let declaration = fs::read(&binding).expect("declaration");
    fs::remove_file(&binding).expect("legacy root");
    let before = payloads(&home);
    assert_eq!(before.len(), 2);
    let columns = reopen(&runtime, &home).expect("adopt history");
    assert_eq!(fs::read(&binding).expect("bound"), declaration);
    assert_eq!(payloads(&home), before);
    put(&columns, "concord", &sealed, "one");
    put(&columns, "concord", &open, "two");
    assert_eq!(records(&columns, &Filter::default()), expected);
    let selected = Filter {
        selector: Some((Role::trace(), Key::new("t0").expect("key"))),
        ..Filter::default()
    };
    assert_eq!(records(&columns, &selected).len(), 3);
    let selected = Filter {
        selector: Some((
            Role::new("locus.span").expect("role"),
            Key::new("s0").expect("key"),
        )),
        ..Filter::default()
    };
    assert_eq!(records(&columns, &selected).len(), 2);
    drop(columns);
    let columns = reopen(&runtime, &home).expect("bound reopen");
    assert_eq!(records(&columns, &Filter::default()), expected);
    put(&columns, "concord", &[atom(BASE + 4, 4)], "three");
    assert_eq!(records(&columns, &Filter::default()).len(), 6);
}

#[test]
fn bindings() {
    let opened = Opened::new("binding-refusal");
    put(&opened.columns, "concord", &atoms(0..3), "one");
    opened.columns.tick(FAR).expect("seal");
    put(&opened.columns, "concord", &atoms(3..5), "two");
    let Opened {
        _runtime: runtime,
        home,
        columns,
    } = opened;
    drop(columns);
    let before = payloads(&home);
    assert_eq!(before.len(), 2);
    let binding = home.join("column/model.json");
    let mut declaration: serde_json::Value =
        serde_json::from_slice(&fs::read(&binding).expect("binding")).expect("model");
    fs::write(&binding, b"invalid model").expect("invalid binding");
    assert!(reopen(&runtime, &home).is_err());
    assert_eq!(payloads(&home), before);
    assert_eq!(fs::read(&binding).expect("unchanged"), b"invalid model");
    declaration["sort"] = serde_json::json!(["producer"]);
    let changed = serde_json::to_vec(&declaration).expect("changed model");
    keel_column::Table::parse(std::str::from_utf8(&changed).expect("utf8"))
        .expect("valid incompatible model");
    fs::write(&binding, &changed).expect("incompatible binding");
    let error = reopen(&runtime, &home).err().expect("refuse drift");
    assert!(error.contains("column model differs"), "{error}");
    assert_eq!(payloads(&home), before);
    assert_eq!(fs::read(&binding).expect("unchanged"), changed);
}

#[test]
fn malformed() {
    let opened = Opened::new("history-refusal");
    put(&opened.columns, "concord", &atoms(0..3), "one");
    let Opened {
        _runtime: runtime,
        home,
        columns,
    } = opened;
    drop(columns);
    fs::remove_file(home.join("column/model.json")).expect("legacy root");
    let files = payloads(&home);
    assert_eq!(files.len(), 1);
    let segment = files
        .keys()
        .find(|path| path.extension().is_some_and(|ext| ext == "seg"))
        .expect("open segment");
    fs::OpenOptions::new()
        .append(true)
        .open(segment)
        .expect("open tail")
        .write_all(b"torn")
        .expect("torn tail");
    let before = payloads(&home);
    assert!(reopen(&runtime, &home).is_err());
    assert_eq!(payloads(&home), before);
    assert!(!home.join("column/model.json").exists());
}

fn reopen(runtime: &Runtime, home: &Path) -> Result<Columns, String> {
    runtime.block_on(Columns::open(home, runtime.handle().clone()))
}

fn records(columns: &Columns, filter: &Filter) -> Vec<Vec<u8>> {
    let mut records = Vec::new();
    columns
        .read(filter, &mut |line| {
            records.push(line.to_vec());
            Ok(())
        })
        .expect("read");
    records.sort();
    records
}

fn payloads(home: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    for area in ["parts", "open"] {
        collect(&home.join("column").join(area), &mut found);
    }
    found
}

fn collect(path: &Path, found: &mut BTreeMap<PathBuf, Vec<u8>>) {
    if path.is_dir() {
        for entry in fs::read_dir(path).expect("directory") {
            collect(&entry.expect("entry").path(), found);
        }
    } else if path.exists() {
        found.insert(path.to_path_buf(), fs::read(path).expect("record file"));
    }
}
