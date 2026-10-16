use crate::store::Producer;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct Expected {
    names: BTreeSet<Producer>,
}

impl Expected {
    pub fn read(home: &Path) -> Result<Self, String> {
        let declared = crate::config::read(home)?;
        let mut names = BTreeSet::new();
        let listed = declared
            .producers
            .map(|section| section.expected)
            .unwrap_or_default();
        for name in listed {
            if !names.insert(Producer::new(&name)?) {
                return Err(format!("producer {name:?} is expected twice"));
            }
        }
        Ok(Self { names })
    }

    pub fn names(&self) -> impl Iterator<Item = &Producer> {
        self.names.iter()
    }
}
