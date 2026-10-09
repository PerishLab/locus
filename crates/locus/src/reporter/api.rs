use super::Report;
use super::spool::{self, Spool};
use crate::{Atom, Error};
use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

const CEILING: u64 = 512 * 1024 * 1024;
const SEGMENT: u64 = 16 * 1024 * 1024;
const AGE: Duration = Duration::from_secs(60);
const MARKER: &str = "registration";
const SCHEME: &str = "http://";
const CONNECT: Duration = Duration::from_millis(100);
const EXCHANGE: Duration = Duration::from_millis(300);

pub(crate) struct Target {
    endpoint: String,
    directory: PathBuf,
}

pub(super) fn build(options: &Value) -> Result<Box<dyn Report>, Error> {
    let object = options
        .as_object()
        .filter(|object| object.len() == 2)
        .ok_or_else(|| Error::config("api reporter options do not match its schema"))?;
    let text = |name: &str| {
        object
            .get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::config(format!("api reporter requires {name}")))
    };
    let endpoint = text("endpoint")?;
    authority(endpoint)?;
    let spool = spool::open(text("path")?, CEILING, SEGMENT)?.aged(AGE);
    Ok(Box::new(Api {
        endpoint: endpoint.trim_end_matches('/').to_string(),
        spool,
    }))
}

struct Api {
    endpoint: String,
    spool: Spool,
}

impl Report for Api {
    fn report(&self, atom: &Atom) -> Result<Option<String>, String> {
        self.spool.report(atom)
    }

    fn target(&self) -> Option<Target> {
        Some(Target {
            endpoint: self.endpoint.clone(),
            directory: self.spool.directory().to_path_buf(),
        })
    }
}

pub(crate) fn enroll(target: &Target, producer: Option<&str>) -> Result<(), String> {
    let marker = target.directory.join(MARKER);
    let known = fs::read_to_string(&marker).is_ok_and(|text| text == target.endpoint);
    let producer = producer.ok_or("the api reporter requires a declared producer")?;
    match post(target, producer) {
        Ok(()) => fs::write(&marker, &target.endpoint)
            .map_err(|error| format!("cannot record registration: {error}")),
        Err(_) if known => Ok(()),
        Err(error) => Err(format!(
            "spool {} is not registered with {}: {error}",
            target.directory.display(),
            target.endpoint
        )),
    }
}

fn post(target: &Target, producer: &str) -> Result<(), String> {
    let authority = authority(&target.endpoint).map_err(|error| error.to_string())?;
    let directory = canonical(&target.directory)?;
    let body = json!({"producer": producer, "path": directory}).to_string();
    let address = authority
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .next()
        .ok_or("endpoint resolves to no address")?;
    let mut stream =
        TcpStream::connect_timeout(&address, CONNECT).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(EXCHANGE))
        .and_then(|_| stream.set_write_timeout(Some(EXCHANGE)))
        .map_err(|error| error.to_string())?;
    let request = format!(
        "POST /api/v1/spools HTTP/1.1\r\nhost: {authority}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| error.to_string())?;
    let mut status = [0_u8; 12];
    stream
        .read_exact(&mut status)
        .map_err(|error| error.to_string())?;
    if status.starts_with(b"HTTP/1.1 2") || status.starts_with(b"HTTP/1.0 2") {
        Ok(())
    } else {
        Err(format!(
            "registration answered {}",
            String::from_utf8_lossy(&status).trim()
        ))
    }
}

fn authority(endpoint: &str) -> Result<&str, Error> {
    endpoint
        .strip_prefix(SCHEME)
        .map(|rest| rest.trim_end_matches('/'))
        .filter(|rest| !rest.is_empty() && !rest.contains('/'))
        .ok_or_else(|| Error::config(format!("api endpoint {endpoint:?} is not http://HOST:PORT")))
}

fn canonical(directory: &Path) -> Result<String, String> {
    fs::canonicalize(directory)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| error.to_string())
}
