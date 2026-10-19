//! Deliberately small HTTP/1.1 subset: bounded headers, fixed-length bodies,
//! one request per connection, no redirects, cookies, CORS or uploaded downloads.
use std::{
    collections::HashMap,
    io::{BufReader, Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};
pub(super) struct Request {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub length: u64,
}
pub(super) fn request(
    input: &mut BufReader<TcpStream>,
    deadline: Instant,
) -> Result<Request, String> {
    let mut budget = 16 * 1024;
    let mut line = || -> Result<String, String> {
        let mut bytes = Vec::new();
        loop {
            if Instant::now() >= deadline || budget == 0 || bytes.len() >= 4096 {
                return Err("invalid_http".into());
            }
            if input.buffer().is_empty() {
                input
                    .get_ref()
                    .set_read_timeout(Some(
                        deadline
                            .saturating_duration_since(Instant::now())
                            .min(Duration::from_millis(500)),
                    ))
                    .map_err(|_| "invalid_http")?;
            }
            let mut byte = [0u8; 1];
            input.read_exact(&mut byte).map_err(|_| "invalid_http")?;
            budget -= 1;
            bytes.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        if !bytes.ends_with(b"\r\n") {
            return Err("invalid_http".into());
        }
        bytes.truncate(bytes.len() - 2);
        if !bytes.iter().all(|b| (32..=126).contains(b)) {
            return Err("invalid_http".into());
        }
        String::from_utf8(bytes).map_err(|_| "invalid_http".into())
    };
    let first = line()?;
    let parts = first.split(' ').collect::<Vec<_>>();
    if parts.len() != 3 || parts[2] != "HTTP/1.1" || !matches!(parts[0], "GET" | "POST") {
        return Err("invalid_http".into());
    }
    let mut headers = HashMap::new();
    loop {
        let value = line()?;
        if value.is_empty() {
            break;
        }
        let (name, value) = value.split_once(':').ok_or("invalid_http")?;
        if name.is_empty()
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || headers.len() >= 64
        {
            return Err("invalid_http".into());
        }
        if headers
            .insert(name.to_ascii_lowercase(), value.trim().to_owned())
            .is_some()
        {
            return Err("invalid_http".into());
        }
    }
    if headers.contains_key("transfer-encoding") || headers.contains_key("expect") {
        return Err("invalid_http".into());
    }
    let length = match headers.get("content-length") {
        Some(value) if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => {
            value.parse::<u64>().map_err(|_| "invalid_http")?
        }
        None if parts[0] == "GET" => 0,
        _ => return Err("invalid_http".into()),
    };
    if parts[0] == "GET" && length != 0 {
        return Err("invalid_http".into());
    }
    input
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|_| "invalid_http")?;
    Ok(Request {
        method: parts[0].into(),
        path: parts[1].into(),
        headers,
        length,
    })
}
pub(super) fn response(stream: &mut TcpStream, status: u16, mime: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        413 => "Content Too Large",
        _ => "Service Unavailable",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: {mime}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\n\r\n",
        body.len()
    );
    let _ = stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.write_all(body));
}
pub(super) fn name(value: &str) -> Result<String, String> {
    if value.len() > 720 {
        return Err("invalid_filename".into());
    }
    let mut bytes = Vec::new();
    let input = value.as_bytes();
    let mut index = 0;
    while index < input.len() {
        if input[index] == b'%' {
            let a = *input.get(index + 1).ok_or("invalid_filename")?;
            let b = *input.get(index + 2).ok_or("invalid_filename")?;
            let a = (a as char).to_digit(16).ok_or("invalid_filename")?;
            let b = (b as char).to_digit(16).ok_or("invalid_filename")?;
            bytes.push((a * 16 + b) as u8);
            index += 3;
        } else {
            bytes.push(input[index]);
            index += 1;
        }
    }
    let name = String::from_utf8(bytes).map_err(|_| "invalid_filename")?;
    if name.is_empty()
        || name.len() > 240
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\'))
        || name == "."
        || name == ".."
    {
        return Err("invalid_filename".into());
    }
    Ok(name)
}
