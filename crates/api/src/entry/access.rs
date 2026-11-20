use clap::{Args, ValueEnum};
use locus_api::column::{Binding, Mode, assign};
use locus_api::store::Producer;
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct Access {
    #[arg(long)]
    home: Option<PathBuf>,
    #[arg(value_enum)]
    action: Action,
    #[arg(long)]
    issuer: String,
    #[arg(long)]
    subject: String,
    #[arg(long)]
    producer: String,
    #[arg(value_enum)]
    mode: Mode,
}

#[derive(Clone, ValueEnum)]
enum Action {
    Grant,
    Revoke,
}

impl Access {
    pub(crate) fn run(self) -> Result<(), String> {
        let home = self
            .home
            .or_else(|| plumb::config::data("locus"))
            .ok_or("no Locus home")?;
        std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
        let producer = Producer::new(&self.producer)?;
        let binding = Binding {
            issuer: self.issuer,
            subject: self.subject,
            grant: matches!(self.action, Action::Grant),
        };
        tokio::runtime::Runtime::new()
            .map_err(|e| e.to_string())?
            .block_on(assign(&home, &binding, &producer, self.mode))
    }
}
