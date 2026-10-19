//! The engine: one thread per core, each with its own event loop (epoll on
//! Linux, kqueue on macOS), its own listening socket (`SO_REUSEPORT`, the
//! kernel spreads the connections) and its own Rythme interpreter. Nothing is
//! shared between threads, nothing is locked on the way of a request.
//!
//! Requests sent one after the other on a connection (pipelining) are all
//! answered in one write. Heads are parsed with httparse, without allocating.

use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use slab::Slab;

use crate::http::{self, Request, Response};

/// What a worker does with the requests it reads.
pub trait Service {
    /// Writes the whole response to `req` (status line, headers, body) at the end of `out`.
    fn handle(&mut self, req: &Request, out: &mut Vec<u8>);
    /// Called after each turn of the event loop (to write the logs, say).
    fn after_turn(&mut self) {}
}

#[derive(Clone)]
pub struct Config {
    pub addr: SocketAddr,
    pub workers: usize,
    /// Largest request head, in bytes.
    pub max_head: usize,
    /// Largest request body, in bytes.
    pub max_body: usize,
    /// A connection without a request for this long is closed.
    pub keep_alive: Duration,
    /// A request must arrive whole within this time (slow clients).
    pub read_timeout: Duration,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            workers: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
            max_head: 16 * 1024,
            max_body: 8 * 1024 * 1024,
            keep_alive: Duration::from_secs(75),
            read_timeout: Duration::from_secs(30),
        }
    }
}

/// Set by SIGTERM (Heroku, Docker) or Ctrl-C: the workers finish and stop.
static STOP: AtomicBool = AtomicBool::new(false);

pub fn stop() {
    STOP.store(true, Ordering::SeqCst);
}

/// Runs `make(worker)` on each worker thread to get its service, then
/// serves until stopped. Returns when every worker stopped.
pub fn serve<F>(config: Config, make: F) -> io::Result<()>
where
    F: Fn(usize) -> Box<dyn Service> + Send + Sync + 'static,
{
    let flag = Arc::new(AtomicBool::new(false));
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        let _ = signal_hook::flag::register(sig, flag.clone());
    }
    {
        let flag = flag.clone();
        std::thread::spawn(move || loop {
            if flag.load(Ordering::Relaxed) {
                STOP.store(true, Ordering::SeqCst);
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        });
    }
    // One socket per worker when the system spreads connections between them;
    // otherwise one socket that every worker listens to.
    let shared = match listener(config.addr, true) {
        Ok(l) => {
            drop(l);
            None
        }
        Err(_) => Some(listener(config.addr, false)?),
    };
    let make = Arc::new(make);
    let mut threads = Vec::new();
    for w in 0..config.workers.max(1) {
        let config = config.clone();
        let make = make.clone();
        let socket = match &shared {
            Some(s) => s.try_clone()?,
            None => listener(config.addr, true)?,
        };
        threads.push(std::thread::Builder::new().name(format!("facile-{w}")).stack_size(crate::STACK_SIZE).spawn(move || {
            crate::runtime::prepare_thread();
            let service = make(w);
            if let Err(e) = run_worker(&config, socket, service) {
                eprintln!("facile: worker {w}: {e}");
            }
        })?);
    }
    for t in threads {
        let _ = t.join();
    }
    Ok(())
}

fn listener(addr: SocketAddr, reuse_port: bool) -> io::Result<std::net::TcpListener> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
    if reuse_port {
        socket.set_reuse_port(true)?;
    }
    #[cfg(not(all(unix, not(any(target_os = "solaris", target_os = "illumos")))))]
    if reuse_port {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "SO_REUSEPORT"));
    }
    socket.set_nodelay(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(4096)?;
    Ok(socket.into())
}

const LISTENER: Token = Token(usize::MAX);

/// How long a worker keeps looking for events after a busy turn.
const SPIN: Duration = Duration::from_micros(15);

struct Conn {
    stream: TcpStream,
    peer: SocketAddr,
    /// Always initialized up to its length; the bytes read are `rbuf[..rlen]`.
    rbuf: Vec<u8>,
    rlen: usize,
    wbuf: Vec<u8>,
    wpos: usize,
    /// The last time a request was read or a byte written.
    last: Instant,
    /// When the request being read started arriving.
    started: Option<Instant>,
    close_after_write: bool,
    writable: bool,
    sent_continue: bool,
}

fn run_worker(config: &Config, socket: std::net::TcpListener, mut service: Box<dyn Service>) -> io::Result<()> {
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(1024);
    let mut listener = TcpListener::from_std(socket);
    poll.registry().register(&mut listener, LISTENER, Interest::READABLE)?;
    let mut conns: Slab<Conn> = Slab::with_capacity(1024);
    let mut req = Request::default();
    let mut last_sweep = Instant::now();
    let mut listening = true;
    let mut busy = false;
    loop {
        if STOP.load(Ordering::Relaxed) {
            if listening {
                let _ = poll.registry().deregister(&mut listener);
                listening = false;
            }
            // Finish what is being written, then stop.
            conns.retain(|_, c| c.wpos < c.wbuf.len());
            if conns.is_empty() {
                return Ok(());
            }
        }
        // Right after a busy turn, more requests are usually on their way:
        // look again for a few microseconds before sleeping, since waking a
        // sleeping thread costs more than these looks (and adds latency).
        let mut found = false;
        if busy {
            let spin = Instant::now();
            while spin.elapsed() < SPIN {
                match poll.poll(&mut events, Some(Duration::ZERO)) {
                    Ok(()) if !events.is_empty() => {
                        found = true;
                        break;
                    }
                    Ok(()) => std::hint::spin_loop(),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(e),
                }
            }
        }
        if !found {
            match poll.poll(&mut events, Some(Duration::from_millis(500))) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        busy = !events.is_empty();
        if busy {
            http::tick();
        }
        for event in events.iter() {
            if event.token() == LISTENER {
                accept(&poll, &mut listener, &mut conns);
                continue;
            }
            let key = event.token().0;
            let Some(conn) = conns.get_mut(key) else { continue };
            let mut alive = true;
            if event.is_writable() || conn.writable {
                alive = flush(conn);
            }
            if alive && (event.is_readable() || event.is_read_closed()) {
                alive = read(conn, config);
            }
            if alive {
                alive = process(conn, config, &mut req, service.as_mut());
            }
            if alive {
                alive = flush(conn);
            }
            if alive {
                alive = interest(&poll, key, conn);
            }
            if !alive || (conn.close_after_write && conn.wpos >= conn.wbuf.len()) {
                let mut c = conns.remove(key);
                let _ = poll.registry().deregister(&mut c.stream);
            }
        }
        service.after_turn();
        let now = Instant::now();
        if now.duration_since(last_sweep) >= Duration::from_secs(1) {
            last_sweep = now;
            let stale: Vec<usize> = conns
                .iter()
                .filter(|(_, c)| match c.started {
                    Some(t) => now.duration_since(t) > config.read_timeout,
                    None => now.duration_since(c.last) > config.keep_alive,
                })
                .map(|(k, _)| k)
                .collect();
            for k in stale {
                let mut c = conns.remove(k);
                let _ = poll.registry().deregister(&mut c.stream);
            }
        }
    }
}

fn accept(poll: &Poll, listener: &mut TcpListener, conns: &mut Slab<Conn>) {
    loop {
        match listener.accept() {
            Ok((mut stream, peer)) => {
                let _ = stream.set_nodelay(true);
                let entry = conns.vacant_entry();
                let token = Token(entry.key());
                if poll.registry().register(&mut stream, token, Interest::READABLE).is_err() {
                    continue;
                }
                entry.insert(Conn {
                    stream,
                    peer,
                    rbuf: vec![0; 4096],
                    rlen: 0,
                    wbuf: Vec::with_capacity(4096),
                    wpos: 0,
                    last: Instant::now(),
                    started: None,
                    close_after_write: false,
                    writable: false,
                    sent_continue: false,
                });
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            // Out of file descriptors, say: try again at the next event.
            Err(_) => return,
        }
    }
}

/// Reads all that arrived. False when the connection is over.
fn read(conn: &mut Conn, config: &Config) -> bool {
    let limit = config.max_head + config.max_body + 64 * 1024;
    loop {
        if conn.rlen >= limit {
            // `process` answers 413 or 431 and closes.
            return true;
        }
        if conn.rbuf.len() - conn.rlen < 2048 {
            let grown = (conn.rbuf.len() * 2).min(limit + 4096);
            conn.rbuf.resize(grown, 0);
        }
        let rlen = conn.rlen;
        match conn.stream.read(&mut conn.rbuf[rlen..]) {
            Ok(0) => {
                // The client closed its side: answer what was read, then close.
                conn.close_after_write = true;
                return conn.rlen > 0 || conn.wpos < conn.wbuf.len();
            }
            Ok(n) => {
                let room = conn.rbuf.len() - conn.rlen;
                conn.rlen += n;
                if conn.started.is_none() {
                    conn.started = Some(Instant::now());
                }
                // Less than the room given: the socket is empty now. Events are
                // edge-triggered, so bytes arriving later wake the loop again:
                // no need for a read that would only say "nothing more".
                if n < room {
                    return true;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return true,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return false,
        }
    }
}

/// Writes what is waiting. False when the connection is over.
fn flush(conn: &mut Conn) -> bool {
    while conn.wpos < conn.wbuf.len() {
        match conn.stream.write(&conn.wbuf[conn.wpos..]) {
            Ok(0) => return false,
            Ok(n) => {
                conn.wpos += n;
                conn.last = Instant::now();
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return true,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return false,
        }
    }
    conn.wbuf.clear();
    conn.wpos = 0;
    true
}

/// Asks for write events only while something waits to be written.
fn interest(poll: &Poll, key: usize, conn: &mut Conn) -> bool {
    let pending = conn.wpos < conn.wbuf.len();
    if pending != conn.writable {
        conn.writable = pending;
        let i = if pending { Interest::READABLE | Interest::WRITABLE } else { Interest::READABLE };
        if poll.registry().reregister(&mut conn.stream, Token(key), i).is_err() {
            return false;
        }
    }
    true
}

/// A response written by the engine itself (malformed requests).
fn engine_response(status: u16, msg: &str, out: &mut Vec<u8>) {
    let mut r = Response::text(status, msg);
    r.set_header("connection", "close");
    r.write_head(out);
    http::write_date(out);
    http::write_length(r.body.len(), out);
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(r.body.as_slice());
}

/// Answers every whole request in the buffer. False when the connection
/// must be closed now.
fn process(conn: &mut Conn, config: &Config, req: &mut Request, service: &mut dyn Service) -> bool {
    let mut start = 0;
    // Stop reading requests while the client does not read the answers.
    while start < conn.rlen && conn.wbuf.len() - conn.wpos < 4 << 20 && !conn.close_after_write_now() {
        let buf = &conn.rbuf[start..conn.rlen];
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut parsed = httparse::Request::new(&mut headers);
        let head_len = match parsed.parse(buf) {
            Ok(httparse::Status::Complete(n)) => n,
            Ok(httparse::Status::Partial) => {
                if buf.len() > config.max_head {
                    engine_response(431, "Request Header Fields Too Large", &mut conn.wbuf);
                    conn.close_after_write = true;
                    conn.rlen = 0;
                    return true;
                }
                break;
            }
            Err(httparse::Error::TooManyHeaders) => {
                engine_response(431, "Request Header Fields Too Large", &mut conn.wbuf);
                conn.close_after_write = true;
                conn.rlen = 0;
                return true;
            }
            Err(_) => {
                engine_response(400, "Bad Request", &mut conn.wbuf);
                conn.close_after_write = true;
                conn.rlen = 0;
                return true;
            }
        };
        if head_len > config.max_head {
            engine_response(431, "Request Header Fields Too Large", &mut conn.wbuf);
            conn.close_after_write = true;
            conn.rlen = 0;
            return true;
        }
        let base = buf.as_ptr() as usize;
        let at = |s: &str| -> (u32, u32) {
            let a = s.as_ptr() as usize - base;
            (a as u32, (a + s.len()) as u32)
        };
        let http10 = parsed.version == Some(0);
        req.head.clear();
        req.head.extend_from_slice(&buf[..head_len]);
        req.method = at(parsed.method.unwrap_or("GET"));
        let target = parsed.path.unwrap_or("/");
        req.target = at(target);
        req.path_end = req.target.0 + target.find('?').unwrap_or(target.len()) as u32;
        req.headers.clear();
        let mut length: Option<usize> = None;
        let mut chunked = false;
        let mut close = http10;
        let mut expect_continue = false;
        for h in parsed.headers.iter() {
            // httparse checked the bytes of names and values; ours are UTF-8 or read as empty.
            let value = std::str::from_utf8(h.value).unwrap_or("");
            let a = h.value.as_ptr() as usize - base;
            req.headers.push((at(h.name), (a as u32, (a + h.value.len()) as u32)));
            if h.name.eq_ignore_ascii_case("content-length") {
                match value.trim().parse::<usize>() {
                    Ok(n) if length.is_none_or(|l| l == n) => length = Some(n),
                    _ => {
                        engine_response(400, "Bad Request", &mut conn.wbuf);
                        conn.close_after_write = true;
                        conn.rlen = 0;
                        return true;
                    }
                }
            } else if h.name.eq_ignore_ascii_case("transfer-encoding") {
                chunked = value.to_ascii_lowercase().contains("chunked");
            } else if h.name.eq_ignore_ascii_case("connection") {
                let v = value.to_ascii_lowercase();
                if v.contains("close") {
                    close = true;
                } else if v.contains("keep-alive") {
                    close = false;
                }
            } else if h.name.eq_ignore_ascii_case("expect") {
                expect_continue = value.eq_ignore_ascii_case("100-continue");
            }
        }
        // Both is how requests are smuggled past proxies: refused.
        if chunked && length.is_some() {
            engine_response(400, "Bad Request", &mut conn.wbuf);
            conn.close_after_write = true;
            conn.rlen = 0;
            return true;
        }
        req.body.clear();
        let consumed;
        if chunked {
            match decode_chunked(&buf[head_len..], &mut req.body, config.max_body) {
                Chunked::Done(n) => consumed = head_len + n,
                Chunked::Partial => {
                    if expect_continue && !conn.sent_continue {
                        conn.wbuf.extend_from_slice(b"HTTP/1.1 100 Continue\r\n\r\n");
                        conn.sent_continue = true;
                    }
                    break;
                }
                Chunked::TooLarge => {
                    engine_response(413, "Content Too Large", &mut conn.wbuf);
                    conn.close_after_write = true;
                    conn.rlen = 0;
                    return true;
                }
                Chunked::Bad => {
                    engine_response(400, "Bad Request", &mut conn.wbuf);
                    conn.close_after_write = true;
                    conn.rlen = 0;
                    return true;
                }
            }
        } else {
            let n = length.unwrap_or(0);
            if n > config.max_body {
                engine_response(413, "Content Too Large", &mut conn.wbuf);
                conn.close_after_write = true;
                conn.rlen = 0;
                return true;
            }
            if buf.len() < head_len + n {
                if expect_continue && !conn.sent_continue {
                    conn.wbuf.extend_from_slice(b"HTTP/1.1 100 Continue\r\n\r\n");
                    conn.sent_continue = true;
                }
                break;
            }
            req.body.extend_from_slice(&buf[head_len..head_len + n]);
            consumed = head_len + n;
        }
        req.http10 = http10;
        req.keep_alive = !close;
        req.peer = Some(conn.peer.ip());
        service.handle(req, &mut conn.wbuf);
        start += consumed;
        conn.sent_continue = false;
        conn.last = Instant::now();
        if close {
            conn.close_after_write = true;
            start = conn.rlen;
        }
    }
    if start > 0 {
        conn.rbuf.copy_within(start..conn.rlen, 0);
        conn.rlen -= start;
    }
    conn.started = if conn.rlen == 0 { None } else { conn.started.or(Some(Instant::now())) };
    true
}

impl Conn {
    fn close_after_write_now(&self) -> bool {
        // After a `Connection: close`, nothing more is read.
        self.close_after_write && self.rlen == 0
    }
}

enum Chunked {
    /// The whole body, and the number of bytes it took.
    Done(usize),
    Partial,
    TooLarge,
    Bad,
}

/// Decodes a `Transfer-Encoding: chunked` body.
fn decode_chunked(buf: &[u8], out: &mut Vec<u8>, max: usize) -> Chunked {
    let mut i = 0;
    loop {
        let Some(eol) = buf[i..].windows(2).position(|w| w == b"\r\n") else {
            return if buf.len() - i > 64 { Chunked::Bad } else { Chunked::Partial };
        };
        let line = &buf[i..i + eol];
        let size_part = line.split(|&b| b == b';').next().unwrap_or(b"");
        let Ok(size_text) = std::str::from_utf8(size_part) else { return Chunked::Bad };
        let Ok(size) = usize::from_str_radix(size_text.trim(), 16) else { return Chunked::Bad };
        i += eol + 2;
        if size == 0 {
            // Trailers, up to an empty line.
            loop {
                let Some(eol) = buf[i..].windows(2).position(|w| w == b"\r\n") else { return Chunked::Partial };
                i += eol + 2;
                if eol == 0 {
                    return Chunked::Done(i);
                }
            }
        }
        if out.len() + size > max {
            return Chunked::TooLarge;
        }
        if buf.len() < i + size + 2 {
            return Chunked::Partial;
        }
        out.extend_from_slice(&buf[i..i + size]);
        if &buf[i + size..i + size + 2] != b"\r\n" {
            return Chunked::Bad;
        }
        i += size + 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_bodies() {
        let mut out = Vec::new();
        assert!(matches!(decode_chunked(b"4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\nGET", &mut out, 100), Chunked::Done(24)));
        assert_eq!(out, b"Wikipedia");
        out.clear();
        assert!(matches!(decode_chunked(b"4\r\nWik", &mut out, 100), Chunked::Partial));
        out.clear();
        assert!(matches!(decode_chunked(b"zz\r\n", &mut out, 100), Chunked::Bad));
        out.clear();
        assert!(matches!(decode_chunked(b"ff\r\n", &mut out, 100), Chunked::TooLarge));
    }
}
