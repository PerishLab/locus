use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

pub(crate) fn serve(body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listen");
    let api = format!("http://{}", listener.local_addr().expect("address"));
    let body = body.to_owned();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut request = Vec::new();
        let mut chunk = [0; 1024];
        while !request.ends_with(b"\r\n\r\n") {
            let read = stream.read(&mut chunk).expect("request");
            if read == 0 {
                return;
            }
            request.extend_from_slice(&chunk[..read]);
        }
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/x-ndjson\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).expect("head");
        stream.write_all(body.as_bytes()).expect("body");
    });
    api
}
