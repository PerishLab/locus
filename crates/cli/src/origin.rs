use clap::Args;
use std::io::{BufRead, BufReader};

#[derive(Args)]
pub struct Origin {
    #[arg(
        long,
        value_name = "URL",
        default_value = "http://127.0.0.1:43308",
        help = "Base URL of the locus-api to read Atoms from"
    )]
    api: String,
    #[arg(
        long,
        value_name = "NANOSECONDS",
        help = "Keep Atoms at or after this instant (ns since epoch)"
    )]
    from: Option<u64>,
    #[arg(
        long,
        value_name = "NANOSECONDS",
        help = "Keep Atoms before this instant (ns since epoch)"
    )]
    to: Option<u64>,
    #[arg(
        long,
        value_name = "ROLE=KEY",
        help = "Keep Atoms whose Context binds ROLE to exactly KEY"
    )]
    select: Option<String>,
    #[arg(long, help = "Keep Atoms from this producer only")]
    producer: Option<String>,
}

const RETAINED: &str = "locus-retained-from";

pub struct Opened {
    pub reader: Box<dyn BufRead>,
    pub retained: Option<u64>,
}

impl Origin {
    pub fn open(&self) -> Result<Opened, String> {
        let api = &self.api;
        let mut request = ureq::get(format!("{}/api/v1/atoms", api.trim_end_matches('/')));
        for (name, value) in self.params()? {
            request = request.query(name, value);
        }
        let response = request
            .config()
            .http_status_as_error(false)
            .build()
            .call()
            .map_err(|error| format!("cannot reach {api}: {error}"))?;
        let status = response.status();
        let retained = response
            .headers()
            .get(RETAINED)
            .map(|value| {
                value
                    .to_str()
                    .ok()
                    .and_then(|text| text.parse().ok())
                    .ok_or_else(|| format!("api sent an invalid {RETAINED} header"))
            })
            .transpose()?;
        let mut body = response.into_body();
        if status != 200 {
            let detail = body.read_to_string().unwrap_or_default();
            return Err(format!("api refused with {status}: {}", detail.trim()));
        }
        Ok(Opened {
            reader: Box::new(BufReader::new(body.into_reader())),
            retained,
        })
    }

    fn params(&self) -> Result<Vec<(&'static str, String)>, String> {
        let mut params = Vec::new();
        if let Some(from) = self.from {
            params.push(("from", from.to_string()));
        }
        if let Some(to) = self.to {
            params.push(("to", to.to_string()));
        }
        if let Some(select) = &self.select {
            let (role, key) = select
                .split_once('=')
                .ok_or_else(|| format!("selector {select:?} is not ROLE=KEY"))?;
            params.push(("role", role.to_string()));
            params.push(("key", key.to_string()));
        }
        if let Some(producer) = &self.producer {
            params.push(("producer", producer.clone()));
        }
        Ok(params)
    }
}
