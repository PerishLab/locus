use crate::transport;
use clap::Args;
use plumb::config::value;
use std::path::PathBuf;
use url::{Host, Url};

#[derive(Args)]
pub(crate) struct Credentials {
    #[arg(
        long,
        value_name = "TOKEN",
        conflicts_with = "credential",
        help = "Explicit bearer token; use --token-file for a protected file"
    )]
    token: Option<String>,
    #[arg(
        long = "token-file",
        value_name = "PATH",
        help = "Read the bearer token for this API from a file"
    )]
    credential: Option<PathBuf>,
}

impl Credentials {
    pub(super) fn resolve(&self, target: &Url) -> Result<Option<String>, String> {
        let configured = value("LOCUS_API");
        let local = loopback(target);
        let default = configured.as_deref().unwrap_or("http://127.0.0.1:43308");
        let bound = transport::reader(default).is_ok_and(|url| url.origin() == target.origin());
        let file = self.credential.clone().or_else(|| {
            (bound && self.token.is_none())
                .then(|| value("LOCUS_API_TOKEN_FILE"))
                .flatten()
                .map(PathBuf::from)
        });
        let token = match file {
            Some(path) => Some(transport::secret(&path)?),
            None => self.token.clone().or_else(|| {
                (bound || (configured.is_none() && local))
                    .then(|| value("LOCUS_API_TOKEN"))
                    .flatten()
            }),
        };
        if token.is_some() && !local && target.scheme() != "https" {
            return Err("bearer credentials require HTTPS or loopback".into());
        }
        Ok(token)
    }
}

fn loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Ipv4(host)) => host.is_loopback(),
        Some(Host::Ipv6(host)) => host.is_loopback(),
        Some(Host::Domain("localhost")) => true,
        _ => false,
    }
}
