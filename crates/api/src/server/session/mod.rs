mod ledger;
mod wire;
use crate::server::identity::{Authority, Owner};
use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use ledger::{Entry, Ledger, now};
use serde::Deserialize;
use std::sync::Arc;
use url::Url;

const FLOW: &str = "__Host-locus-flow";
const SESSION: &str = "__Host-locus-session";

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Browser {
    pub client: String,
    pub origin: String,
}

impl Browser {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let url = Url::parse(&self.origin).map_err(|_| "invalid browser origin")?;
        crate::server::identity::issuer(&self.origin)?;
        if self.origin != url.origin().ascii_serialization()
            || self.client.is_empty()
            || self.client == "locus"
        {
            return Err("browser needs a canonical origin and distinct OIDC client".into());
        }
        Ok(())
    }

    fn callback(&self) -> String {
        format!("{}/auth/callback", self.origin)
    }
}

#[derive(Clone)]
pub(crate) struct Sessions {
    browser: Browser,
    authority: Arc<Authority>,
    ledger: Ledger,
}

impl Sessions {
    pub(crate) fn new(browser: Browser, authority: Arc<Authority>) -> Self {
        Self {
            browser,
            authority,
            ledger: Ledger::open(),
        }
    }

    pub(crate) fn routes(&self) -> Router {
        Router::new()
            .route("/auth/login", get(login))
            .route("/auth/callback", get(callback))
            .route("/auth/logout", post(logout))
            .with_state(self.clone())
    }

    pub(crate) async fn owner(&self, headers: &HeaderMap) -> Result<Owner, StatusCode> {
        let key = wire::cookie(headers, SESSION).ok_or(StatusCode::UNAUTHORIZED)?;
        let Some(Entry::Person(subject)) = self.ledger.get(key).await? else {
            return Err(StatusCode::UNAUTHORIZED);
        };
        Ok(Owner {
            issuer: self.authority.issuer().into(),
            subject,
        })
    }
}

async fn login(State(session): State<Sessions>) -> Result<Response, StatusCode> {
    let state = wire::secret()?;
    let verifier = wire::secret()?;
    let nonce = wire::secret()?;
    let challenge =
        radicula_oidc::pkce::challenge(&verifier).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut url = Url::parse(&format!("{}/authorize", session.authority.issuer()))
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    url.query_pairs_mut().extend_pairs([
        ("response_type", "code"),
        ("resource", "locus"),
        ("scope", "openid"),
        ("client_id", &session.browser.client),
        ("redirect_uri", &session.browser.callback()),
        ("state", &state),
        ("nonce", &nonce),
        ("code_challenge_method", "S256"),
        ("code_challenge", &challenge),
    ]);
    session
        .ledger
        .put(
            state.clone(),
            now()? + 300,
            Entry::Pending { verifier, nonce },
        )
        .await?;
    wire::redirect(url.as_str(), FLOW, &state, 300)
}

#[derive(Deserialize)]
struct Callback {
    code: String,
    state: String,
}

async fn callback(
    State(session): State<Sessions>,
    headers: HeaderMap,
    Query(args): Query<Callback>,
) -> Result<Response, StatusCode> {
    let key = wire::cookie(&headers, FLOW)
        .filter(|v| **v == args.state)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let Some(Entry::Pending { verifier, nonce }) = session.ledger.take(key).await? else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let browser = session.browser.clone();
    let authority = session.authority.clone();
    let login = tokio::task::spawn_blocking(move || {
        wire::trade(&authority, &browser, &args.code, (verifier, nonce))
    })
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)??;
    if let Some(prior) = wire::cookie(&headers, SESSION) {
        session.ledger.take(prior).await?;
    }
    let sid = wire::secret()?;
    session
        .ledger
        .put(sid.clone(), login.1, Entry::Person(login.0))
        .await?;
    let mut response = wire::redirect("/", SESSION, &sid, login.1.saturating_sub(now()?))?;
    response
        .headers_mut()
        .append(header::SET_COOKIE, wire::bake(FLOW, "", 0)?);
    Ok(response)
}

async fn logout(
    State(session): State<Sessions>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    if headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(&session.browser.origin) {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(sid) = wire::cookie(&headers, SESSION) {
        session.ledger.take(sid).await?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .append(header::SET_COOKIE, wire::bake(SESSION, "", 0)?);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    Ok(response)
}
