use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct Trace {
    pub records: u64,
    pub from: u64,
    pub until: u64,
    pub parent: Option<String>,
    pub conflicting: bool,
}

pub enum Owner {
    One(String),
    Many,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Links {
    pub resolved: u64,
    pub missing: u64,
    pub ambiguous: u64,
    pub conflicting: u64,
}

impl Links {
    pub fn absorb(&mut self, other: Self) {
        self.resolved += other.resolved;
        self.missing += other.missing;
        self.ambiguous += other.ambiguous;
        self.conflicting += other.conflicting;
    }
}

enum Link {
    Resolved(String),
    Missing,
    Ambiguous,
    Conflicting,
    Absent,
}

pub struct Forest<'a> {
    traces: &'a BTreeMap<String, Trace>,
    spans: &'a BTreeMap<String, Owner>,
    children: BTreeMap<&'a str, Vec<&'a str>>,
}

pub struct Grove {
    pub root: String,
    pub members: BTreeSet<String>,
    pub links: Links,
    pub detached: bool,
}

impl<'a> Forest<'a> {
    pub fn new(traces: &'a BTreeMap<String, Trace>, spans: &'a BTreeMap<String, Owner>) -> Self {
        let mut forest = Self {
            traces,
            spans,
            children: BTreeMap::new(),
        };
        for name in traces.keys() {
            if let Link::Resolved(parent) = forest.link(name) {
                let parent = traces.get_key_value(&parent).map(|(key, _)| key.as_str());
                if let Some(parent) = parent {
                    forest.children.entry(parent).or_default().push(name);
                }
            }
        }
        forest
    }

    fn link(&self, name: &str) -> Link {
        let Some(trace) = self.traces.get(name) else {
            return Link::Absent;
        };
        if trace.conflicting {
            return Link::Conflicting;
        }
        let Some(parent) = &trace.parent else {
            return Link::Absent;
        };
        match self.spans.get(parent) {
            Some(Owner::One(owner)) if owner != name => Link::Resolved(owner.clone()),
            Some(Owner::Many) => Link::Ambiguous,
            _ => Link::Missing,
        }
    }

    pub fn grove(&self, seed: &str) -> Grove {
        let root = self.climb(seed);
        let mut members = BTreeSet::new();
        let mut stack = vec![root.as_str()];
        while let Some(name) = stack.pop() {
            if members.insert(name.to_string()) {
                stack.extend(self.children.get(name).into_iter().flatten().copied());
            }
        }
        let mut links = Links::default();
        for name in &members {
            match self.link(name) {
                Link::Resolved(_) => links.resolved += 1,
                Link::Missing => links.missing += 1,
                Link::Ambiguous => links.ambiguous += 1,
                Link::Conflicting => links.conflicting += 1,
                Link::Absent => {}
            }
        }
        let detached = !matches!(self.link(&root), Link::Absent | Link::Resolved(_));
        Grove {
            root,
            members,
            links,
            detached,
        }
    }

    fn climb(&self, seed: &str) -> String {
        let mut path: Vec<String> = Vec::new();
        let mut current = seed.to_string();
        while !path.contains(&current) {
            path.push(current.clone());
            match self.link(&current) {
                Link::Resolved(parent) => current = parent,
                _ => return current,
            }
        }
        let start = path.iter().position(|name| *name == current).unwrap_or(0);
        path.split_off(start).into_iter().min().unwrap_or(current)
    }
}
