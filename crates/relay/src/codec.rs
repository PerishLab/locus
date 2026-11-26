use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, StatusCode, header};
use flate2::read::MultiGzDecoder;
use std::io::Read;

const COMPRESSED: usize = 128 * 1024;
const PLAIN: usize = 256 * 1024;

enum Encoding {
    Plain,
    Gzip,
}

pub(crate) async fn read(headers: &HeaderMap, body: Body) -> Result<Vec<u8>, StatusCode> {
    let mode = encoding(headers)?;
    let limit = match mode {
        Encoding::Plain => PLAIN,
        Encoding::Gzip => COMPRESSED,
    };
    let bytes = to_bytes(body, limit).await.map_err(failure)?;
    match mode {
        Encoding::Plain => Ok(bytes.to_vec()),
        Encoding::Gzip => inflate(&bytes),
    }
}

fn encoding(headers: &HeaderMap) -> Result<Encoding, StatusCode> {
    let mut encodings = headers.get_all(header::CONTENT_ENCODING).iter();
    let Some(value) = encodings.next() else {
        return Ok(Encoding::Plain);
    };
    if encodings.next().is_some() {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let name = value
        .to_str()
        .map_err(|_| StatusCode::UNSUPPORTED_MEDIA_TYPE)?
        .trim()
        .to_ascii_lowercase();
    match name.as_str() {
        "identity" => Ok(Encoding::Plain),
        "gzip" => Ok(Encoding::Gzip),
        _ => Err(StatusCode::UNSUPPORTED_MEDIA_TYPE),
    }
}

fn inflate(body: &[u8]) -> Result<Vec<u8>, StatusCode> {
    let mut reader = MultiGzDecoder::new(body).take(PLAIN as u64 + 1);
    let mut output = Vec::new();
    reader
        .read_to_end(&mut output)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    if output.len() > PLAIN {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    Ok(output)
}

fn failure(error: axum::Error) -> StatusCode {
    if error.to_string().contains("length limit exceeded") {
        StatusCode::PAYLOAD_TOO_LARGE
    } else {
        StatusCode::BAD_REQUEST
    }
}
