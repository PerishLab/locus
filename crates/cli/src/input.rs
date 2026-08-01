use locus::Atom;
use std::io::BufRead;

pub fn scan(
    mut reader: impl BufRead,
    mut observe: impl FnMut(Atom, u64) -> Result<(), String>,
) -> Result<u64, String> {
    let mut offset = 0_u64;
    loop {
        let mut line = Vec::new();
        let bytes = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("cannot read line {}: {error}", offset + 1))?;
        if bytes == 0 {
            return Ok(offset);
        }
        offset = offset
            .checked_add(1)
            .ok_or_else(|| "Atom count overflowed".to_string())?;
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.is_empty() {
            return Err(format!("invalid Atom on line {offset}: empty record"));
        }
        let atom = serde_json::from_slice(&line)
            .map_err(|error| format!("invalid Atom on line {offset}: {error}"))?;
        let encoded = u64::try_from(bytes)
            .map_err(|_| format!("encoded size overflowed on line {offset}"))?;
        observe(atom, encoded)?;
    }
}
