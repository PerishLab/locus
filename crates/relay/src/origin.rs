use crate::Error;
use axum::http::{HeaderMap, StatusCode, header};
use url::Url;

pub(crate) fn parse(origins: Vec<String>) -> Result<Vec<String>, Error> {
    let mut cleaned = Vec::with_capacity(origins.len());
    for raw in origins {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.ends_with('/') {
            return Err(Error::new("relay origin must be a bare scheme://host"));
        }
        let url = Url::parse(trimmed).map_err(|_| Error::new("invalid relay origin"))?;
        if !bare(&url) {
            return Err(Error::new("relay origin must be scheme+host+port only"));
        }
        cleaned.push(url.origin().ascii_serialization());
    }
    Ok(cleaned)
}

fn bare(url: &Url) -> bool {
    let transport = url.has_host() && matches!(url.scheme(), "http" | "https");
    let anonymous = url.username().is_empty() && url.password().is_none();
    let plain = url.path() == "/" && url.query().is_none() && url.fragment().is_none();
    transport && anonymous && plain
}

pub(crate) fn check(headers: &HeaderMap, allowed: &[String]) -> Result<(), StatusCode> {
    let mut sites = headers.get_all("sec-fetch-site").iter();
    if let Some(value) = sites.next() {
        if sites.next().is_some() {
            return Err(StatusCode::FORBIDDEN);
        }
        let site = value
            .to_str()
            .map_err(|_| StatusCode::FORBIDDEN)?
            .trim()
            .to_ascii_lowercase();
        if site != "same-origin" {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    if !allowed.is_empty() {
        let mut origins = headers.get_all(header::ORIGIN).iter();
        let value = origins.next().ok_or(StatusCode::FORBIDDEN)?;
        if origins.next().is_some() {
            return Err(StatusCode::FORBIDDEN);
        }
        let origin = value.to_str().map_err(|_| StatusCode::FORBIDDEN)?.trim();
        if !allowed.iter().any(|entry| entry == origin) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(())
}
