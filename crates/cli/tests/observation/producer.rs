use super::{Fixture, input};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

#[test]
fn https() {
    let fixture = Fixture::new();
    let key = fixture.seat().join("writer");
    std::fs::write(&key, "fixture\n").unwrap();
    fixture.configure(&format!(
        "[locus]\nenabled=true\nendpoint='https://127.0.0.1:0'\nkey_file={}\n",
        serde_json::to_string(&key).unwrap()
    ));
    let result = fixture.run(&["query", "locus.trace", "private-key"], &input());
    assert!(result.status.success());
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("private-key"));
    let queue = fixture.seat().join("state/locus.https/locus");
    let db = Connection::open_with_flags(queue.join("queue.db"), OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    let mut query = db
        .prepare("SELECT body FROM records ORDER BY sequence")
        .unwrap();
    let atoms: Vec<Value> = query
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .unwrap()
        .map(|row| serde_json::from_slice(&row.unwrap()).unwrap())
        .collect();
    assert!(atoms.iter().all(|atom| atom["producer"] == "locus"));
    for event in ["cli.start", "cli.finish"] {
        assert!(atoms.iter().any(|atom| atom["payload"]["event"] == event));
    }
    assert!(!fixture.seat().join("state/locus").exists());
}
