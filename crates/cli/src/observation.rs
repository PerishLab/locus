use plumb::observation::{Declaration, Run};

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
