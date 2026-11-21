use std::fs::File;
use std::io::Read;
use std::path::Path;
use ureq::tls::{RootCerts, TlsConfig};
use url::Url;

pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .http_status_as_error(false)
        .max_redirects(0)
        .proxy(None)
        .build()
        .new_agent()
}

pub fn reader(text: &str) -> Result<Url, String> {
    let url = Url::parse(text).map_err(|_| "invalid API URL")?;
    let absolute = url.has_host() && matches!(url.scheme(), "http" | "https");
    let credentials = !url.username().is_empty() || url.password().is_some();
    let parameters = url.query().is_some() || url.fragment().is_some();
    if !absolute || credentials || parameters {
        return Err("invalid API URL".into());
    }
    Ok(url)
}

pub fn origin(text: &str) -> Result<Url, String> {
    let url = reader(text)?;
    if url.path() != "/" {
        return Err("delivery requires an origin without a path".into());
    }
    Ok(url)
}

pub fn secret(path: &Path) -> Result<String, String> {
    let mut text = String::new();
    File::open(path)
        .and_then(|file| file.take(8193).read_to_string(&mut text))
        .map_err(|_| "cannot read key file".to_string())?;
    let key = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(&text);
    if text.len() > 8192 || key.is_empty() || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err("key file must contain one nonempty token line".into());
    }
    Ok(key.to_owned())
}
