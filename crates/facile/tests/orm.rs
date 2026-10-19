//! The ORM on each database: memory://, SQLite, and PostgreSQL when
//! FACILE_TEST_POSTGRES gives its URL (`postgres://user:pass@localhost/db`).

use std::path::PathBuf;
use std::sync::Once;

use facile::runtime::{self, Global, Runtime};
use facile::testing::{parse_response, request};

fn app() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/apps/orm/app.ry")
}

fn global() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let file = app();
        runtime::set_global(Global { root: file.parent().unwrap().into(), entry: file, production: false, lang: "en".into() });
    });
}

/// Runs `f` with the application on the database `url`, on a large stack.
fn with_app(url: &str, f: impl FnOnce(&mut Runtime) + Send + 'static) {
    global();
    let url = url.to_string();
    std::thread::Builder::new()
        .stack_size(facile::STACK_SIZE)
        .spawn(move || {
            runtime::prepare_thread();
            let mut rt = Runtime::load(&app(), false).expect("the application loads");
            rt.settings.database = Some(url);
            rt.start(true).expect("the database opens");
            f(&mut rt);
        })
        .unwrap()
        .join()
        .unwrap();
}

fn get(rt: &mut Runtime, target: &str) -> (u16, String) {
    let (s, _, b) = parse_response(&rt.respond(&request("GET", target, &[], "")));
    (s, b)
}

fn send(rt: &mut Runtime, method: &str, target: &str, body: &str) -> (u16, String) {
    let (s, _, b) = parse_response(&rt.respond(&request(method, target, &[("content-type", "application/json")], body)));
    (s, b)
}

fn scenario(url: &str) {
    with_app(url, |rt| {
        let (status, body) = get(rt, "/scenario");
        assert_eq!((status, body.as_str()), (200, "ok"), "{body}");
        // The REST API of `resource`.
        let (s, b) = send(rt, "POST", "/api/taches", r#"{"titre": "Par l'API", "priorite": 2}"#);
        assert_eq!(s, 201, "{b}");
        let id: i64 = serde_json::from_str::<serde_json::Value>(&b).unwrap()["id"].as_i64().unwrap();
        let (s, b) = get(rt, &format!("/api/taches/{id}"));
        assert!(s == 200 && b.contains("Par l'API"), "{s} {b}");
        let (s, b) = send(rt, "PATCH", &format!("/api/taches/{id}"), r#"{"fait": true}"#);
        assert!(s == 200 && b.contains("\"fait\":true"), "{s} {b}");
        let (s, b) = send(rt, "POST", "/api/taches", r#"{"titre": ""}"#);
        assert!(s == 422 && b.contains("titre"), "{s} {b}");
        let (s, b) = get(rt, "/api/taches?fait=true&order=-id&limit=1");
        assert!(s == 200 && b.contains("Par l'API"), "{s} {b}");
        let (s, b) = get(rt, "/api/taches?page=1&per_page=2");
        assert!(s == 200 && b.contains("\"per_page\":2"), "{s} {b}");
        let (s, _) = send(rt, "DELETE", &format!("/api/taches/{id}"), "");
        assert_eq!(s, 204);
        let (s, _) = get(rt, &format!("/api/taches/{id}"));
        assert_eq!(s, 404);
    });
}

#[test]
fn memory() {
    scenario("memory://");
}

#[test]
fn sqlite() {
    let file = std::env::temp_dir().join(format!("facile-orm-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&file);
    scenario(&format!("sqlite://{}", file.display()));
    // A second start finds the tables and keeps them.
    scenario(&format!("sqlite://{}", file.display()));
    with_app(&format!("sqlite://{}", file.display()), |rt| {
        let (s, b) = get(rt, "/sql");
        assert!(s == 200 && b.contains("\"n\":"), "{s} {b}");
    });
    let _ = std::fs::remove_file(&file);
}

#[test]
fn postgres() {
    let Ok(url) = std::env::var("FACILE_TEST_POSTGRES") else {
        eprintln!("FACILE_TEST_POSTGRES is not set: PostgreSQL not tested");
        return;
    };
    scenario(&url);
    scenario(&url);
}
