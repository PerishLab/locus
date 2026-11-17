pub(super) struct Shape {
    pub overrides: Vec<&'static str>,
    pub wrapper: &'static str,
    pub program: &'static str,
    pub operation: &'static str,
    pub reason: &'static str,
}

impl Shape {
    pub fn opaque(reason: &'static str) -> Self {
        Self {
            overrides: Vec::new(),
            wrapper: "opaque",
            program: "other",
            operation: "other",
            reason,
        }
    }

    pub fn read(mut words: &[String]) -> Self {
        let wrapped =
            matches!(words, [program, seat, ..] if program == "runseal" && seat.starts_with(':'));
        if wrapped {
            words = &words[2..];
        }
        let mut overrides = Vec::new();
        let env = words.first().is_some_and(|word| word == "env");
        if env {
            words = &words[1..];
            while let Some((word, rest)) = words.split_first() {
                let Some((name, _)) = word.split_once('=') else {
                    break;
                };
                if !assignment(name) {
                    return Self::opaque("env-unparsed");
                }
                overrides.push(if word == "GODEBUG=http2client=0" {
                    "GODEBUG=http2client=0"
                } else {
                    "other"
                });
                words = rest;
            }
        }
        let Some((program, args)) = words.split_first() else {
            return Self::opaque("program-unresolved");
        };
        let Some(program) = resolve(program) else {
            return Self::opaque("program-unresolved");
        };
        let Some(operation) = operation(program, args) else {
            return Self::opaque("operation-unresolved");
        };
        Self {
            overrides,
            wrapper: match (wrapped, env) {
                (true, true) => "runseal-env",
                (true, false) => "runseal",
                (false, true) => "env",
                (false, false) => "none",
            },
            program,
            operation,
            reason: "known",
        }
    }
}

fn resolve(program: &str) -> Option<&'static str> {
    match program {
        "gh" => Some("gh"),
        "concord" => Some("concord"),
        "plumb" => Some("plumb"),
        "santi" => Some("santi"),
        _ => None,
    }
}

fn operation(program: &str, args: &[String]) -> Option<&'static str> {
    if program == "santi" {
        let words: Vec<_> = std::iter::once(program.to_owned())
            .chain(args.iter().cloned())
            .collect();
        return super::tool::operation(&words);
    }
    match (program, args.first().map(String::as_str)) {
        (_, Some("--help" | "-h")) => Some("help"),
        (_, Some("--version" | "-V")) => Some("version"),
        ("gh", Some("api")) => Some("api"),
        ("gh", Some("issue")) => Some("issue"),
        ("gh", Some("pr")) => Some("pr"),
        ("concord", Some("issue")) => Some("issue"),
        ("concord", Some("acceptance")) => Some("acceptance"),
        ("concord", Some("member")) => Some("member"),
        ("plumb", Some("release")) => Some("release"),
        ("plumb", Some("land")) => Some("land"),
        ("plumb", Some("metadata")) => Some("metadata"),
        _ => None,
    }
}

fn assignment(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
