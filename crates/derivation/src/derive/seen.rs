use locus::Atom;
use std::collections::{BTreeMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, Eq, PartialEq)]
struct Digest {
    length: usize,
    forward: u64,
    reverse: u64,
}

#[derive(Default)]
pub(crate) struct Seen {
    entries: BTreeMap<(Option<String>, String), Digest>,
}

impl Seen {
    pub(crate) fn admit(&mut self, atom: &Atom) -> Result<bool, String> {
        let Some(id) = atom.id().filter(|value| !value.is_empty()) else {
            return Ok(true);
        };
        let key = (atom.producer().map(str::to_owned), id.to_owned());
        let digest = fingerprint(atom)?;
        match self.entries.get(&key) {
            Some(prior) if *prior == digest => Ok(false),
            Some(_) => Err(format!("conflicting Atom identity {id}")),
            None => {
                self.entries.insert(key, digest);
                Ok(true)
            }
        }
    }
}

fn fingerprint(atom: &Atom) -> Result<Digest, String> {
    let bytes = serde_json::to_vec(atom).map_err(|error| format!("cannot encode Atom: {error}"))?;
    let mut forward = DefaultHasher::new();
    bytes.hash(&mut forward);
    let mut reverse = DefaultHasher::new();
    for byte in bytes.iter().rev() {
        byte.hash(&mut reverse);
    }
    Ok(Digest {
        length: bytes.len(),
        forward: forward.finish(),
        reverse: reverse.finish(),
    })
}
