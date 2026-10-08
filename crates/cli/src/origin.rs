use clap::Args;
use std::io::{self, BufRead, BufReader};

#[derive(Args)]
pub struct Origin {
    #[arg(
        long,
        value_name = "URL",
        help = "Read Atoms from a locus-api at this base URL instead of stdin"
    )]
    api: Option<String>,
    #[arg(
        long,
        value_name = "NANOSECONDS",
        requires = "api",
        help = "With --api, keep Atoms at or after this instant (ns since epoch)"
    )]
    from: Option<u64>,
    #[arg(
        long,
        value_name = "NANOSECONDS",
        requires = "api",
        help = "With --api, keep Atoms before this instant (ns since epoch)"
    )]
    to: Option<u64>,
    #[arg(
        long,
        value_name = "ROLE=KEY",
        requires = "api",
        help = "With --api, keep Atoms whose Context binds ROLE to exactly KEY"
    )]
    select: Option<String>,
    #[arg(
        long,
        requires = "api",
        help = "With --api, keep Atoms from this producer only"
    )]
    producer: Option<String>,
}

impl Origin {
    pub fn open(&self) -> Result<Box<dyn BufRead>, String> {
        let Some(api) = &self.api else {
            return Ok(Box::new(io::stdin().lock()));
        };
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
        let mut body = response.into_body();
        if status != 200 {
            let detail = body.read_to_string().unwrap_or_default();
            return Err(format!("api refused with {status}: {}", detail.trim()));
        }
        Ok(Box::new(BufReader::new(body.into_reader())))
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
