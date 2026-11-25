use facile::http::{self, Request};
use facile::server::{self, Service};

struct Hello;
impl Service for Hello {
    fn handle(&mut self, req: &Request, out: &mut Vec<u8>) {
        out.extend_from_slice(b"HTTP/1.1 200 OK\r\ncontent-type: text/plain; charset=utf-8\r\n");
        http::write_date(out);
        if !req.keep_alive {
            out.extend_from_slice(b"connection: close\r\n");
        }
        http::write_length(13, out);
        out.extend_from_slice(b"\r\nHello, World!");
    }
}
fn main() {
    let c = server::Config {
        addr: "127.0.0.1:3100".parse().unwrap(),
        workers: std::env::var("W").ok().and_then(|w| w.parse().ok()).unwrap_or(2),
        ..server::Config::default()
    };
    server::serve(c, |_| Box::new(Hello)).unwrap();
}
