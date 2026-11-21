mod credentials;
mod incomplete;
pub(crate) use credentials::Credentials;
pub(crate) mod instant;

use clap::Args;
use incomplete::Bounds;
use serde::Serialize;
use std::io::{BufRead, BufReader};

#[derive(Args, Serialize)]
pub struct Origin {
    #[arg(
        long,
        value_name = "URL",
        default_value = "http://127.0.0.1:43308",
        env = "LOCUS_API",
        hide_env_values = true,
        help = "Base URL of the locus-api to read Atoms from"
    )]
    api: String,
    #[arg(
        long,
        value_name = "INSTANT",
        value_parser = crate::origin::instant::parse,
        help = "Keep Atoms at or after this instant: ns since epoch, or RFC3339 with a zone such as 2026-10-17T21:00:00+08:00"
    )]
    from: Option<u64>,
    #[arg(
        long,
        value_name = "INSTANT",
        value_parser = crate::origin::instant::parse,
        help = "Keep Atoms before this instant: ns since epoch, or RFC3339 with a zone such as 2026-10-17T13:00:00Z"
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
    #[serde(skip)]
    #[command(flatten)]
    credentials: Credentials,
}

const RETAINED: &str = "locus-retained-from";
const REFUSALS: [&str; 3] = ["request.unauthorized", "request.invalid", "route.missing"];

pub struct Opened {
    pub reader: Box<dyn BufRead>,
    pub retained: Option<u64>,
}

impl Origin {
    pub fn slice(
        api: String,
        window: locus_derivation::quality::Window,
        credentials: Credentials,
        producer: &str,
    ) -> Self {
        Self {
            api,
            from: Some(window.from),
            to: Some(window.to),
            select: None,
            producer: Some(producer.into()),
            credentials,
        }
    }

    #[locus::trace(with = plumb::observation::view())]
    pub fn open(&self) -> Result<Opened, String> {
        let mut api = crate::transport::reader(&self.api)?;
        let token = self.credentials.resolve(&api)?;
        api.path_segments_mut()
            .map_err(|_| "invalid API URL")?
            .pop_if_empty()
            .extend(["api", "v1", "atoms"]);
        let mut request = crate::transport::agent().get(api.as_str());
        for (name, value) in self.params()? {
            request = request.query(name, value);
        }
        if let Some(token) = &token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let response = request.call().map_err(|_| "cannot reach API".to_string())?;
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
            if let Some(code) = refusal(&detail) {
                plumb::observation::fault(code);
            }
            return Err(match (&token, refusal(&detail)) {
                (Some(_), Some(code)) => format!("api refused with {status}: {code}"),
                (Some(_), None) => format!("api refused with {status}"),
                _ => format!("api refused with {status}: {}", detail.trim()),
            });
        }
        Ok(Opened {
            reader: Box::new(BufReader::new(body.into_reader())),
            retained,
        })
    }

    pub fn bounds(&self, retained: Option<u64>) -> Bounds {
        Bounds {
            from: self.from,
            to: self.to,
            retained,
        }
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

fn refusal(detail: &str) -> Option<&'static str> {
    let value: serde_json::Value = serde_json::from_str(detail).ok()?;
    let code = value.get("fault")?.get("code")?.as_str()?;
    REFUSALS.into_iter().find(|known| *known == code)
}
