use axum::http::{StatusCode, Uri};
use radicula_oidc::exchange;
use radicula_oidc::jwt::{Algorithm, DecodingKey, Rules, Verifier};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ensign {
    pub listen: SocketAddr,
    pub issuer: String,
    pub keys: PathBuf,
}

pub struct Authority {
    issuer: String,
    keys: Vec<Verifier>,
    client: ureq::Agent,
}

#[derive(Deserialize)]
struct Keys {
    keys: Vec<Key>,
}

#[derive(Deserialize)]
struct Key {
    kty: String,
    kid: String,
    alg: String,
    crv: String,
    #[serde(rename = "use")]
    usage: String,
    x: String,
    y: String,
}

pub(crate) fn issuer(value: &str) -> Result<(), String> {
    let uri: Uri = value.parse().map_err(|_| "invalid Ensign issuer")?;
    let host = uri.host().ok_or("Ensign issuer needs a host")?;
    let local = host
        .trim_matches(['[', ']'])
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback());
    let private = uri.authority().is_some_and(|x| x.as_str().contains('@'));
    let suffix = uri.query().is_some() || value.ends_with('/') || value.contains('#');
    let transport = match uri.scheme_str() {
        Some("https") => true,
        Some("http") => local,
        _ => false,
    };
    if private || suffix || !transport {
        return Err("Ensign issuer must be the exact HTTPS issuer or an HTTP loopback IP".into());
    }
    Ok(())
}

impl Authority {
    pub fn load(home: &Path) -> Result<Option<(SocketAddr, Self)>, String> {
        let Some(config) = crate::config::read(home)?.ensign else {
            return Ok(None);
        };
        issuer(&config.issuer)?;
        let bytes = std::fs::read(home.join(config.keys)).map_err(|e| e.to_string())?;
        let keys: Keys = serde_json::from_slice(&bytes).map_err(|_| "invalid Ensign JWKS")?;
        let mut seen = BTreeSet::new();
        let mut trusted = Vec::new();
        for key in keys.keys {
            if key.kty != "EC" || key.crv != "P-256" {
                return Err("Ensign signing key must be a P-256 EC key".into());
            }
            if key.alg != "ES256" || key.usage != "sig" {
                return Err("Ensign key must be declared for ES256 signatures".into());
            }
            if key.kid.is_empty() || !seen.insert(key.kid.clone()) {
                return Err("Ensign signing keys need distinct nonempty identifiers".into());
            }
            trusted.push(Verifier {
                algorithm: Algorithm::ES256,
                kid: key.kid,
                key: DecodingKey::from_ec_components(&key.x, &key.y)
                    .map_err(|_| "invalid Ensign key")?,
            });
        }
        if trusted.is_empty() {
            return Err("Ensign JWKS is empty".into());
        }
        let client = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .http_status_as_error(false)
                .max_redirects(0)
                .timeout_global(Some(Duration::from_secs(10)))
                .build(),
        );
        Ok(Some((
            config.listen,
            Self {
                issuer: config.issuer,
                keys: trusted,
                client,
            },
        )))
    }

    pub(crate) fn exchange(&self, key: &str) -> Result<Owner, StatusCode> {
        let mut reply = self
            .client
            .post(format!("{}/token", self.issuer))
            .send_form([
                ("grant_type", exchange::GRANT),
                ("subject_token", key),
                ("subject_token_type", exchange::ACCESS),
                ("resource", "locus"),
            ])
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        match reply.status().as_u16() {
            200 => (),
            400 | 401 | 403 => return Err(StatusCode::UNAUTHORIZED),
            _ => return Err(StatusCode::SERVICE_UNAVAILABLE),
        }
        let token: exchange::Response = reply
            .body_mut()
            .with_config()
            .limit(65536)
            .read_json()
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        token
            .validate()
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
            .as_secs();
        let rules = Rules {
            issuer: &self.issuer,
            audience: "locus",
            subject: None,
            now,
            leeway: 0,
            required: &[("kind", "access")],
        };
        self.keys
            .iter()
            .find_map(|key| key.verify(token.token.expose(), &rules).ok())
            .map(|c| Owner {
                issuer: c.iss,
                subject: c.sub,
            })
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}

pub(crate) struct Owner {
    pub issuer: String,
    pub subject: String,
}
