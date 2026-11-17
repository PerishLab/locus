mod ledger;
mod model;
mod pairs;
#[cfg(test)]
mod tests;

pub use ledger::Quality;
pub use model::{Attempt, Buffer, Fact, Issue, Progress, Report, Side, Stage, Window};

pub const LIMIT: usize = 10_000;
pub const BYTES: usize = 16 * 1024 * 1024;

pub fn key(text: &str) -> bool {
    !text.is_empty() && text.len() <= 512 && !text.chars().any(char::is_control)
}
