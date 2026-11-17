use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Command, Output};

pub fn atom(event: &str, id: &str) -> Value {
    let mut context = json!({"codex.session":"session"});
    if matches!(event, "PreToolUse" | "PostToolUse") {
        context["codex.turn"] = json!("turn");
        context["codex.tool"] = json!("tool");
    }
    json!({"at":100,"id":id,"producer":"locus-adapter-codex","context":context,"choices":[],
        "payload":{"schema":"locus.adapter.codex/v1","event":event,"tool_class":"unclassified"}})
}

pub fn attempt(id: &str, atom: &str) -> Value {
    json!({"schema":"locus.adapter.attempt/v1","at":100,"id":id,"group":"session",
        "event":"PreToolUse","atom":atom,"outcome":{"status":"accepted","buffer":"written"},"issues":[]})
}

pub fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_locus"));
    command.args(["quality", "codex", "--from", "0", "--to", "200"]);
    command
}

pub fn run(body: &str, command: &mut Command) -> Output {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    command.args([
        "--api",
        &format!("http://{}", listener.local_addr().unwrap()),
    ]);
    let body = body.to_owned();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut first = String::new();
        reader.read_line(&mut first).unwrap();
        assert!(first.contains("producer=locus-adapter-codex"));
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
        }
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nlocus-retained-from: 1\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    let result = command.output().unwrap();
    server.join().unwrap();
    result
}
