use super::observe;
use serde_json::{Map, json};

fn fields(command: &str) -> Map<String, serde_json::Value> {
    let mut fields = Map::new();
    observe(
        &json!({"tool_name":"Bash","tool_input":{"command":command}}),
        &mut fields,
    );
    fields
}

#[test]
fn structure() {
    for command in [
        "santi strand send id hello;printf done",
        "santi strand send id hello&&printf done",
        "santi strand send id hello||printf done",
        "santi strand send id hello|cat",
        "santi strand send id hello>output",
        "santi strand send id hello<input",
        "santi strand send id hello&",
        "santi strand send id hello\nprintf done",
        "santi strand send id hello # comment",
    ] {
        let result = fields(command);
        assert_eq!(result["tool_class"], "unclassified", "{command}");
        assert_eq!(result["classification_reason"], "shell-structure");
        assert!(result.get("santi_operation").is_none());
    }
}

#[test]
fn expansion() {
    for command in [
        "santi strand send id $(printf hello)",
        "santi strand send id `printf hello`",
        "santi strand send id \"$(printf hello)\"",
        "santi strand send id $TASK",
        "santi strand send id *.txt",
        "santi strand send id {one,two}",
        "santi strand send id <(printf hello)",
    ] {
        let result = fields(command);
        assert_eq!(result["tool_class"], "unclassified", "{command}");
        assert!(result.get("santi_operation").is_none());
    }
}

#[test]
fn literals() {
    for command in [
        "santi strand send id 'hello;world'",
        "santi strand send id \"hello;world\"",
        "santi strand send id '$(printf hello)'",
        "santi strand send id hello\\;world",
        "santi strand send id \"\\$TASK\"",
        "runseal :santi.soul.source santi strand send id hello",
        "/usr/bin/santi strand send id hello",
    ] {
        let result = fields(command);
        assert_eq!(result["tool_class"], "santi", "{command}");
        assert_eq!(result["santi_operation"], "strand.send");
        assert!(result.get("command").is_none());
    }
}

#[test]
fn incomplete() {
    for command in ["santi strand send id 'hello", "santi health\\"] {
        let result = fields(command);
        assert_eq!(result["tool_class"], "unclassified");
        assert_eq!(result["classification_reason"], "shell-unparsed");
    }
}
