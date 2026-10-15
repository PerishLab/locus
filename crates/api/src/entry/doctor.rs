use crate::home::Home;
use locus_api::access::Readers;
use locus_api::retention;
use serde::Serialize;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Warn,
    Fail,
    Offline,
}

#[derive(Debug, Serialize)]
pub struct Check {
    name: &'static str,
    status: Status,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    next: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    target: &'static str,
    state: &'static str,
    version: String,
    home: PathBuf,
    listen: SocketAddr,
    checks: Vec<Check>,
}

impl Check {
    pub fn new(name: &'static str, status: Status, detail: String, next: Option<&str>) -> Self {
        Self {
            name,
            status,
            detail,
            next: next.map(str::to_string),
        }
    }
}

impl Report {
    pub fn failed(&self) -> bool {
        self.checks.iter().any(|check| check.status == Status::Fail)
    }
}

pub fn run(version: &str, home: &Path, listen: SocketAddr) -> Report {
    let place = Home::new(home);
    let mut checks = vec![
        binary(version, listen),
        place.estate(),
        place.column(),
        place.spools(),
        place.config(),
    ];
    let protected = Readers::read(home).is_ok_and(|readers| !readers.open());
    checks.extend(live(listen, protected));
    let state = if checks.iter().any(|check| check.status == Status::Fail) {
        "failing"
    } else {
        "ready"
    };
    Report {
        target: "local_home",
        state,
        version: version.to_string(),
        home: home.to_path_buf(),
        listen,
        checks,
    }
}

fn binary(version: &str, listen: SocketAddr) -> Check {
    if listen.ip().is_loopback() {
        Check::new(
            "binary",
            Status::Ok,
            format!("locus-api {version}; {listen} is loopback"),
            None,
        )
    } else {
        Check::new(
            "binary",
            Status::Fail,
            format!("locus-api {version}; {listen} is not a loopback address, so serve refuses it"),
            Some("Pass a loopback --listen such as 127.0.0.1:43308, the same address serve uses."),
        )
    }
}

fn live(listen: SocketAddr, protected: bool) -> Vec<Check> {
    let base = format!("http://{listen}");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let health = match agent.get(format!("{base}/health")).call() {
        Ok(response) if response.status() == 200 => Check::new(
            "health",
            Status::Ok,
            format!("{base}/health answered 200"),
            None,
        ),
        Ok(response) => Check::new(
            "health",
            Status::Fail,
            format!("{base}/health answered {}", response.status()),
            Some(
                "Inspect the locus-api service log; the process listening there is not a healthy locus-api.",
            ),
        ),
        Err(error) => {
            return vec![Check::new(
                "health",
                Status::Offline,
                format!("locus-api is not running at {base}: {error}"),
                Some(
                    "Start locus-api serve with this same --home and --listen when the service should be live.",
                ),
            )];
        }
    };
    if health.status != Status::Ok {
        return vec![health];
    }
    let now = retention::now();
    let atoms = match agent
        .get(format!("{base}/api/v1/atoms"))
        .query("from", now.to_string())
        .query("to", now.to_string())
        .call()
    {
        Ok(response) if response.status() == 200 => Check::new(
            "atoms",
            Status::Ok,
            format!("{base}/api/v1/atoms answered 200"),
            None,
        ),
        Ok(response) if protected && response.status() == 401 => Check::new(
            "atoms",
            Status::Ok,
            format!("{base}/api/v1/atoms answered 401 without a token; declared readers guard it"),
            None,
        ),
        Ok(response) => {
            let status = response.status();
            let detail = response.into_body().read_to_string().unwrap_or_default();
            Check::new(
                "atoms",
                Status::Fail,
                format!("{base}/api/v1/atoms answered {status}: {}", detail.trim()),
                Some("Inspect the locus-api service log for store read errors."),
            )
        }
        Err(error) => Check::new(
            "atoms",
            Status::Fail,
            format!("{base}/api/v1/atoms failed: {error}"),
            Some("Inspect the locus-api service log for store read errors."),
        ),
    };
    vec![health, atoms]
}
