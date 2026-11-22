use super::Browser;
use crate::server::identity::Authority;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

pub(super) fn secret() -> Result<String, StatusCode> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub(super) fn cookie<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
    let mut matches = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|v| v.trim().split_once('='))
        .filter(|(name, _)| *name == key);
    let (_, value) = matches.next()?;
    (matches.next().is_none() && value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .then_some(value)
}

pub(super) fn bake(name: &str, value: &str, age: u64) -> Result<HeaderValue, StatusCode> {
    HeaderValue::from_str(&format!(
        "{name}={value}; Path=/; Max-Age={age}; HttpOnly; Secure; SameSite=Lax"
    ))
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub(super) fn redirect(
    location: &str,
    name: &str,
    value: &str,
    age: u64,
) -> Result<Response, StatusCode> {
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::LOCATION,
        HeaderValue::from_str(location).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    headers.insert(header::SET_COOKIE, bake(name, value, age)?);
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    Ok(response)
}

#[derive(Deserialize)]
struct Tokens {
    #[serde(rename = "access_token")]
    access: String,
    #[serde(rename = "id_token")]
    identity: String,
    #[serde(rename = "token_type")]
    kind: String,
}

pub(super) fn trade(
    authority: &Authority,
    browser: &Browser,
    code: &str,
    proof: (String, String),
) -> Result<(String, u64), StatusCode> {
    let mut reply = authority
        .client()
        .post(format!("{}/token", authority.issuer()))
        .send_form([
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", &browser.client),
            ("redirect_uri", &browser.callback()),
            ("code_verifier", &proof.0),
        ])
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !reply.status().is_success() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let tokens: Tokens = reply
        .body_mut()
        .with_config()
        .limit(65536)
        .read_json()
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    if !tokens.kind.eq_ignore_ascii_case("Bearer") {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let id = authority.verify(
        &tokens.identity,
        &browser.client,
        &[("kind", "id"), ("nonce", &proof.1)],
    )?;
    let access = authority.verify(&tokens.access, "locus", &[("kind", "access")])?;
    if !single(&id.aud, &browser.client) || !single(&access.aud, "locus") {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if id.sub != access.sub || access.extra.get("scope").and_then(|v| v.as_str()) != Some("openid")
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok((access.sub, access.exp.min(id.exp)))
}

fn single(audience: &radicula_oidc::jwt::Audience, name: &str) -> bool {
    matches!(audience, radicula_oidc::jwt::Audience::One(value) if value == name)
}
