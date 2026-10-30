use super::{emit, open, temp};
use radicula_lock::{Mode, TryLockError};
use std::{
    fs::OpenOptions,
    io::{Seek, SeekFrom, Write},
    panic::{AssertUnwindSafe, catch_unwind},
};

#[test]
fn unwind() {
    let directory = temp("scope-unwind");
    let spool = open(directory.to_str().unwrap(), 1 << 30, 4096).unwrap();
    let peer = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join("spool.lock"))
        .unwrap();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = spool.with(|_| {
                assert!(matches!(
                    radicula_lock::attempt(&peer, Mode::Exclusive),
                    Err(TryLockError::WouldBlock)
                ));
                panic!("business append unwinds");
            });
        }))
        .is_err()
    );
    radicula_lock::attempt(&peer, Mode::Exclusive).unwrap();
    radicula_lock::unlock(&peer).unwrap();
    assert!(spool.lock.is_poisoned());
    assert_eq!(
        spool
            .with(|_| panic!("poisoned spool must refuse"))
            .unwrap_err(),
        "spool lock is poisoned"
    );
    let gate = spool.lock.lock().unwrap_or_else(|error| error.into_inner());
    assert_eq!(gate.status().active, 0);
    assert!(gate.status().fault.is_none());
}

#[test]
fn failure() {
    let directory = temp("scope-error");
    let spool = open(directory.to_str().unwrap(), 1 << 30, 4096).unwrap();
    assert_eq!(
        spool.with(|_| Err("append failure".into())).unwrap_err(),
        "append failure"
    );
    assert!(!spool.lock.is_poisoned());
    let peer = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join("spool.lock"))
        .unwrap();
    radicula_lock::attempt(&peer, Mode::Exclusive).unwrap();
    radicula_lock::unlock(&peer).unwrap();
    emit(&spool, "after-failure", 0);
}

#[test]
fn cursor() {
    let directory = temp("scope-cursor");
    let spool = open(directory.to_str().unwrap(), 1 << 30, 4096).unwrap();
    let original = {
        let gate = spool.lock.lock().unwrap();
        let mut file = gate.file();
        file.write_all(b"42").unwrap();
        file.seek(SeekFrom::Start(1)).unwrap();
        file as *const std::fs::File
    };
    spool
        .with(|mut file| {
            assert_eq!(file as *const std::fs::File, original);
            assert_eq!(file.stream_position().unwrap(), 1);
            file.seek(SeekFrom::Start(2)).unwrap();
            Ok(None)
        })
        .unwrap();
    let gate = spool.lock.lock().unwrap();
    assert_eq!(gate.file().stream_position().unwrap(), 2);
}

#[test]
fn diagnostics() {
    for output in [
        Err("append failure".to_string()),
        Ok(Some("loss notice".to_string())),
    ] {
        let expected = match &output {
            Err(message) | Ok(Some(message)) => message.clone(),
            Ok(None) => unreachable!(),
        };
        let message = super::super::failure(radicula_lock::Failure::Release {
            output,
            error: radicula_lock::Error::Uncertain(std::sync::Arc::new(std::io::Error::other(
                "cleanup failure",
            ))),
        });
        assert!(message.contains(&expected));
        assert!(message.contains("cannot unlock spool"));
        assert!(message.contains("cleanup failure"));
    }
}
