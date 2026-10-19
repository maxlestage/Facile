//! Requests and responses, as the engine reads and writes them.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

/// A request whose head was read: its bytes are copied once, and the method,
/// the target and the headers are positions in them (no allocation each).
#[derive(Default)]
pub struct Request {
    pub head: Vec<u8>,
    pub method: (u32, u32),
    pub target: (u32, u32),
    /// Where the path ends in the target (the query starts after a `?`).
    pub path_end: u32,
    pub headers: Vec<((u32, u32), (u32, u32))>,
    pub body: Vec<u8>,
    pub http10: bool,
    pub keep_alive: bool,
    pub peer: Option<std::net::IpAddr>,
}

impl Request {
    /// A header value: bytes that are not UTF-8 read as empty.
    #[inline]
    fn slice(&self, (a, b): (u32, u32)) -> &str {
        std::str::from_utf8(&self.head[a as usize..b as usize]).unwrap_or("")
    }

    /// The method, the target and the header names: httparse gave them as
    /// `&str` (it only accepts ASCII there), and `head` is a copy of those bytes.
    #[inline]
    fn token(&self, (a, b): (u32, u32)) -> &str {
        let bytes = &self.head[a as usize..b as usize];
        debug_assert!(std::str::from_utf8(bytes).is_ok());
        // SAFETY: see above; `Request`s built elsewhere (tests) use valid UTF-8 too,
        // and a wrong position would at worst cut a character (checked in debug builds).
        unsafe { std::str::from_utf8_unchecked(bytes) }
    }

    pub fn method(&self) -> &str {
        self.token(self.method)
    }

    pub fn target(&self) -> &str {
        self.token(self.target)
    }

    pub fn target_bytes(&self) -> &[u8] {
        &self.head[self.target.0 as usize..self.target.1 as usize]
    }

    pub fn path(&self) -> &str {
        let t = self.target();
        &t[..(self.path_end - self.target.0) as usize]
    }

    pub fn query(&self) -> &str {
        if self.path_end < self.target.1 {
            &self.target()[(self.path_end - self.target.0 + 1) as usize..]
        } else {
            ""
        }
    }

    /// The first header of this name (names are compared without case).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| self.token(*n).eq_ignore_ascii_case(name)).map(|(_, v)| self.slice(*v))
    }

    pub fn header_pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.headers.iter().map(|(n, v)| (self.token(*n), self.slice(*v)))
    }

    pub fn cookie(&self, name: &str) -> Option<String> {
        let all = self.header("cookie")?;
        all.split(';').find_map(|c| {
            let (k, v) = c.split_once('=')?;
            (k.trim() == name).then(|| percent_decode(v.trim().trim_matches('"')))
        })
    }

    /// The client's address: the first of `X-Forwarded-For` behind a proxy
    /// (Heroku's router sets it), the socket's otherwise.
    pub fn client_ip(&self) -> String {
        if let Some(f) = self.header("x-forwarded-for") {
            if let Some(first) = f.split(',').next() {
                let first = first.trim();
                if !first.is_empty() {
                    return first.to_string();
                }
            }
        }
        self.peer.map(|p| p.to_string()).unwrap_or_else(|| "0.0.0.0".into())
    }

    pub fn accepts_gzip(&self) -> bool {
        self.header("accept-encoding").is_some_and(|e| e.contains("gzip"))
    }
}

/// A response body: shared bytes are given again without a copy.
#[derive(Clone)]
pub enum Body {
    Empty,
    /// A text made by Rythme code, given without a copy.
    Text(rythme::value::Text),
    Bytes(Vec<u8>),
    Shared(std::rc::Rc<[u8]>),
    Static(&'static [u8]),
}

impl Body {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Body::Empty => &[],
            Body::Text(t) => t.as_bytes(),
            Body::Bytes(b) => b,
            Body::Shared(b) => b,
            Body::Static(b) => b,
        }
    }

    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A header name or value: most are written in the code, given without a copy.
pub type HeaderText = std::borrow::Cow<'static, str>;

#[derive(Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(HeaderText, HeaderText)>,
    pub body: Body,
}

impl Response {
    pub fn new(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Response {
        Response {
            status,
            headers: vec![(HeaderText::Borrowed("content-type"), HeaderText::Borrowed(content_type))],
            body: Body::Bytes(body.into()),
        }
    }

    pub fn text(status: u16, s: impl Into<String>) -> Response {
        Response::new(status, "text/plain; charset=utf-8", s.into().into_bytes())
    }

    pub fn html(status: u16, s: impl Into<String>) -> Response {
        Response::new(status, "text/html; charset=utf-8", s.into().into_bytes())
    }

    pub fn json(status: u16, s: impl Into<String>) -> Response {
        Response::new(status, "application/json", s.into().into_bytes())
    }

    pub fn empty(status: u16) -> Response {
        Response { status, headers: Vec::new(), body: Body::Empty }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| &**v)
    }

    pub fn set_header(&mut self, name: &str, value: impl Into<HeaderText>) {
        let value = value.into();
        match self.headers.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some(h) => h.1 = value,
            None => self.headers.push((HeaderText::Owned(name.to_ascii_lowercase()), value)),
        }
    }

    pub fn add_header(&mut self, name: &str, value: impl Into<HeaderText>) {
        self.headers.push((HeaderText::Owned(name.to_ascii_lowercase()), value.into()));
    }

    /// The status line and the headers, without `Date`, `Content-Length`,
    /// `Connection` and the empty line: what a cached response keeps.
    pub fn write_head(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(b"HTTP/1.1 ");
        out.extend_from_slice(&itoa3(self.status));
        out.push(b' ');
        out.extend_from_slice(reason(self.status).as_bytes());
        out.extend_from_slice(b"\r\n");
        for (k, v) in &self.headers {
            // Names and values were checked where they came from the
            // application (`with_header`, response records): no line breaks.
            // The engine writes these itself.
            if matches!(&**k, "content-length" | "connection" | "date" | "transfer-encoding") {
                continue;
            }
            out.extend_from_slice(k.as_bytes());
            out.extend_from_slice(b": ");
            out.extend_from_slice(v.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
    }
}

fn itoa3(n: u16) -> [u8; 3] {
    let n = n.clamp(100, 999);
    [b'0' + (n / 100) as u8, b'0' + (n / 10 % 10) as u8, b'0' + (n % 10) as u8]
}

/// Writes `Content-Length: n\r\n`.
pub fn write_length(len: usize, out: &mut Vec<u8>) {
    out.extend_from_slice(b"content-length: ");
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    let mut n = len;
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.extend_from_slice(&buf[i..]);
    out.extend_from_slice(b"\r\n");
}

pub fn reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        411 => "Length Required",
        413 => "Content Too Large",
        414 => "URI Too Long",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Content",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Unknown",
    }
}

thread_local! {
    /// `date: ...\r\n` for the current second, written once a second.
    static DATE: RefCell<(u64, Vec<u8>)> = const { RefCell::new((0, Vec::new())) };
    /// The current second, read once per turn of the event loop (reading
    /// the clock for each response costs more than writing the response).
    static NOW: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn clock() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Reads the clock for the responses of this turn of the event loop.
pub fn tick() {
    NOW.with(|n| n.set(clock()));
}

/// Writes the `Date` header line of now.
pub fn write_date(out: &mut Vec<u8>) {
    let secs = match NOW.with(|n| n.get()) {
        0 => clock(),
        s => s,
    };
    DATE.with(|d| {
        let mut d = d.borrow_mut();
        if d.0 != secs || d.1.is_empty() {
            d.0 = secs;
            d.1 = format!("date: {}\r\n", http_date(secs)).into_bytes();
        }
        out.extend_from_slice(&d.1);
    });
}

/// A date as HTTP writes it: `Sun, 06 Nov 1994 08:49:37 GMT`.
pub fn http_date(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (h, m, s) = (rem / 3600, rem / 60 % 60, rem % 60);
    let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][(days % 7) as usize];
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    let month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][(mo - 1) as usize];
    let mut out = String::with_capacity(29);
    let _ = write!(out, "{weekday}, {d:02} {month} {y} {h:02}:{m:02}:{s:02} GMT");
    out
}

/// `%41` and `+` in a URL part, as text.
pub fn percent_decode(s: &str) -> String {
    decode(s).into_owned()
}

/// Like `percent_decode`, without a copy when there is nothing to decode.
pub fn decode(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains(['%', '+']) {
        return std::borrow::Cow::Borrowed(s);
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = |c: u8| (c as char).to_digit(16);
                match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    (Some(a), Some(b)) => {
                        out.push((a * 16 + b) as u8);
                        i += 2;
                    }
                    _ => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    std::borrow::Cow::Owned(String::from_utf8_lossy(&out).into_owned())
}

/// `a=1&b=deux` as pairs.
pub fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (percent_decode(k), percent_decode(v)),
            None => (percent_decode(p), String::new()),
        })
        .collect()
}

/// The media type of a file, by its extension.
pub fn content_type(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "webmanifest" => "application/manifest+json",
        "wasm" => "application/wasm",
        "ry" | "txt" | "md" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "pdf" => "application/pdf",
        "xml" => "application/xml",
        "csv" => "text/csv; charset=utf-8",
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        _ => "application/octet-stream",
    }
}

/// Whether gzip makes this kind of content smaller.
pub fn compressible(content_type: &str) -> bool {
    content_type.starts_with("text/")
        || content_type.starts_with("application/json")
        || content_type.starts_with("application/javascript")
        || content_type.starts_with("application/manifest")
        || content_type.starts_with("application/xml")
        || content_type.starts_with("application/wasm")
        || content_type.starts_with("image/svg")
        || content_type.starts_with("model/gltf+json")
}

pub fn gzip(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::GzEncoder::new(Vec::with_capacity(data.len() / 3), flate2::Compression::new(6));
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(http_date(784111777), "Sun, 06 Nov 1994 08:49:37 GMT");
        assert_eq!(http_date(0), "Thu, 01 Jan 1970 00:00:00 GMT");
    }

    #[test]
    fn decoding() {
        assert_eq!(percent_decode("a%20b+c"), "a b c");
        assert_eq!(percent_decode("%"), "%");
        assert_eq!(percent_decode("%e2%82%ac"), "€");
        assert_eq!(parse_query("a=1&b=&c"), vec![("a".into(), "1".into()), ("b".into(), "".into()), ("c".into(), "".into())]);
    }
}
