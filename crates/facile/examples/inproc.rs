//! Times the handling of one request inside the process (no network):
//! `cargo run --release --example inproc -- bench/app.ry /users/42 [POST body]`.

use std::path::PathBuf;
use std::time::Instant;

use facile::http::Request;
use facile::runtime::{self, Global, Runtime};
use facile::server::Service;

#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn request(method: &str, target: &str, body: &str) -> Request {
    let head = format!(
        "{method} {target} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let mut headers = [httparse::EMPTY_HEADER; 16];
    let mut p = httparse::Request::new(&mut headers);
    p.parse(head.as_bytes()).unwrap();
    let base = head.as_ptr() as usize;
    let at = |s: &str| ((s.as_ptr() as usize - base) as u32, (s.as_ptr() as usize - base + s.len()) as u32);
    let mut r = Request { head: head.as_bytes().to_vec(), ..Request::default() };
    r.method = at(p.method.unwrap());
    let t = p.path.unwrap();
    r.target = at(t);
    r.path_end = r.target.0 + t.find('?').unwrap_or(t.len()) as u32;
    for h in p.headers.iter() {
        let v = std::str::from_utf8(h.value).unwrap();
        r.headers.push((at(h.name), at(v)));
    }
    r.body = body.as_bytes().to_vec();
    r.keep_alive = true;
    r
}

fn main() {
    let mut args = std::env::args().skip(1);
    let file = PathBuf::from(args.next().expect("app.ry"));
    let target = args.next().unwrap_or_else(|| "/".into());
    let body = args.next();
    let file = file.canonicalize().unwrap();
    runtime::set_global(Global { root: file.parent().unwrap().into(), entry: file.clone(), production: true, lang: "en".into() });
    std::thread::Builder::new()
        .stack_size(facile::STACK_SIZE)
        .spawn(move || {
            runtime::prepare_thread();
            let mut rt = Runtime::load(&file, false).unwrap();
            rt.start(true).unwrap();
            let req = match &body {
                Some(b) => request("POST", &target, b),
                None => request("GET", &target, ""),
            };
            let mut out = Vec::with_capacity(4096);
            let n = 2_000_000;
            for _ in 0..1000 {
                out.clear();
                rt.handle(&req, &mut out);
            }
            println!("{}", String::from_utf8_lossy(&out));
            // The best of 7 rounds: the others were slowed by the machine.
            let mut best = f64::MAX;
            for _ in 0..7 {
                let t = Instant::now();
                for _ in 0..n / 7 {
                    out.clear();
                    rt.handle(&req, &mut out);
                }
                best = best.min(t.elapsed().as_nanos() as f64 / (n / 7) as f64);
            }
            println!("{best:.0} ns per request");
        })
        .unwrap()
        .join()
        .unwrap();
}
