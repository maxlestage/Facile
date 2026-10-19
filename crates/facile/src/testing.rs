//! Requests made by hand, to test an application without the network.

use crate::http::Request;

/// A request as the engine would read it: `request("POST", "/api/taches",
/// &[("content-type", "application/json")], "{...}")`.
pub fn request(method: &str, target: &str, headers: &[(&str, &str)], body: &str) -> Request {
    let mut head = format!("{method} {target} HTTP/1.1\r\nhost: localhost\r\ncontent-length: {}\r\n", body.len());
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let mut parsed_headers = [httparse::EMPTY_HEADER; 64];
    let mut p = httparse::Request::new(&mut parsed_headers);
    p.parse(head.as_bytes()).expect("a valid request");
    let base = head.as_ptr() as usize;
    let at = |s: &[u8]| ((s.as_ptr() as usize - base) as u32, (s.as_ptr() as usize - base + s.len()) as u32);
    let mut r = Request { head: head.as_bytes().to_vec(), ..Request::default() };
    r.method = at(p.method.unwrap_or("GET").as_bytes());
    let t = p.path.unwrap_or("/");
    r.target = at(t.as_bytes());
    r.path_end = r.target.0 + t.find('?').unwrap_or(t.len()) as u32;
    for h in p.headers.iter() {
        r.headers.push((at(h.name.as_bytes()), at(h.value)));
    }
    r.body = body.as_bytes().to_vec();
    r.keep_alive = true;
    r
}

/// The status and the body of a response the engine wrote.
pub fn parse_response(bytes: &[u8]) -> (u16, Vec<(String, String)>, String) {
    let text = String::from_utf8_lossy(bytes);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let mut lines = head.lines();
    let status = lines.next().and_then(|l| l.split(' ').nth(1)).and_then(|s| s.parse().ok()).unwrap_or(0);
    let headers = lines.filter_map(|l| l.split_once(": ")).map(|(k, v)| (k.to_ascii_lowercase(), v.to_string())).collect();
    (status, headers, body.to_string())
}
