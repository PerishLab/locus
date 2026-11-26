use locus_derivation::locus::{self, Candidate, Config, Context, Engine, Policy, Role};
use plumb::observation::{Declaration, Run};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

const BUFFER: u64 = 16;

#[derive(Default, Deserialize)]
struct File {
    #[serde(default)]
    locus: Settings,
}

#[derive(Default, Deserialize)]
struct Settings {
    #[serde(default)]
    enabled: bool,
    #[serde(rename = "serve_buffer_mib")]
    buffer: Option<u64>,
}

pub(crate) fn start(command: &str) -> Option<Run> {
    let home = plumb::config::data("locus")?;
    Run::start(
        Declaration {
            producer: "locus",
            file: &home.join("config.toml"),
            buffer: home.join("state/locus"),
        },
        command,
    )
}

pub(crate) struct Watch {
    engine: Engine,
    context: Context,
}

impl Watch {
    pub(crate) fn start(buffer: PathBuf) -> Option<Self> {
        let home = plumb::config::data("locus")?;
        let built = settings(&home)
            .and_then(|settings| settings.map(|settings| watch(buffer, settings)).transpose());
        isolate(built)
    }

    pub(crate) fn view(&self) -> (&Engine, &Context) {
        (&self.engine, &self.context)
    }
}

pub(crate) fn note(view: Option<(&Engine, &Context)>, frame: Option<&Context>, payload: Value) {
    if let (Some((engine, _)), Some(frame)) = (view, frame) {
        let _ = engine.append(frame, Candidate::event(payload));
    }
}

fn isolate<T>(built: Result<Option<T>, String>) -> Option<T> {
    built.unwrap_or_else(|error| {
        eprintln!("locus observation: {error}");
        None
    })
}

fn settings(home: &std::path::Path) -> Result<Option<Settings>, String> {
    let path = home.join("config.toml");
    let file: File = match radicula_config::load(&path) {
        Ok(file) => file,
        Err(radicula_config::Error::Read { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            return Ok(None);
        }
        Err(radicula_config::Error::Parse { path, .. }) => {
            return Err(format!(
                "invalid observation configuration in {}; check [locus] settings",
                path.display()
            ));
        }
        Err(error) => return Err(error.to_string()),
    };
    Ok(Some(file.locus).filter(|settings| settings.enabled))
}

fn watch(buffer: PathBuf, settings: Settings) -> Result<Watch, String> {
    let spec = locus::reporter::Spec::local(buffer, settings.buffer.unwrap_or(BUFFER));
    let engine = Engine::bootstrap(Config::new(
        Policy::default().producer("locus").reporter(spec),
    ))
    .map_err(|error| error.to_string())?;
    let candidate = Candidate::event(json!({"event": "serve.start"})).explicit(
        Role::new("locus.command").map_err(|error| error.to_string())?,
        locus::Key::new("api.serve").map_err(|error| error.to_string())?,
    );
    let context = engine
        .append(&Context::empty(), candidate)
        .map_err(|error| error.to_string())?
        .context();
    Ok(Watch { engine, context })
}
