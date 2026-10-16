pub mod access;
pub mod expected;
pub mod retention;

use serde::Deserialize;
use std::path::Path;

const FILE: &str = "locus-api.toml";

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Declared {
    pub(crate) retention: Option<Section>,
    #[serde(default)]
    pub(crate) reader: Vec<Reader>,
    pub(crate) producers: Option<Producers>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Section {
    pub(crate) days: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Producers {
    pub(crate) expected: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reader {
    pub(crate) name: String,
    pub(crate) digest: String,
}

pub(crate) fn read(home: &Path) -> Result<Declared, String> {
    let path = home.join(FILE);
    if !path.is_file() {
        return Ok(Declared::default());
    }
    plumb::config::load(&path).map_err(|error| error.to_string())
}
