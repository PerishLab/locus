use super::Limits;
use std::io::Read;

pub const LINE: usize = 1024 * 1024;

#[locus::trace(with = plumb::observation::view())]
pub fn read(reader: impl Read, limits: &Limits) -> Result<Vec<u8>, String> {
    let mut reader = reader;
    let mut bytes = Vec::new();
    let mut block = [0; 64 * 1024];
    loop {
        let count = reader
            .read(&mut block)
            .map_err(|error| format!("cannot capture API read: {error}"))?;
        if count == 0 {
            break;
        }
        if count as u64 > limits.bytes.saturating_sub(bytes.len() as u64) {
            return Err("analysis input exceeds --max-bytes".into());
        }
        bytes.extend_from_slice(&block[..count]);
    }
    if !bytes.is_empty() && bytes.last() != Some(&b'\n') {
        return Err("analysis input ends without an API record newline".into());
    }
    let mut count = 0;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        count += 1;
        if count > limits.records || line.len() > LINE {
            return Err("analysis input exceeds record count or 1 MiB line limit".into());
        }
    }
    Ok(bytes)
}
