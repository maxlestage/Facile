//! Rythme's tasks (`async fn`, `await`) in a Facile application: they run
//! on other threads, with Facile's functions and the same database, and a
//! response that awaited a task is not kept by the cache of purity.

use std::path::PathBuf;

use facile::runtime::{self, Global, Runtime};
use facile::testing::{parse_response, request};

fn app() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/apps/tasks/app.ry")
}

fn call(rt: &mut Runtime, method: &str, target: &str) -> (u16, String) {
    let (s, _, b) = parse_response(&rt.respond(&request(method, target, &[("content-type", "application/json")], "{}")));
    (s, b)
}

#[test]
fn handlers_await_tasks_that_use_the_database() {
    let file = app();
    runtime::set_global(Global { root: file.parent().unwrap().into(), entry: file, production: false, lang: "en".into() });
    std::thread::Builder::new()
        .stack_size(facile::STACK_SIZE)
        .spawn(|| {
            runtime::prepare_thread();
            let mut rt = Runtime::load(&app(), false).expect("the application loads");
            rt.settings.database = Some("memory://".into());
            rt.start(true).expect("the database opens");

            let (s, b) = call(&mut rt, "GET", "/parallel");
            assert_eq!((s, b.as_str()), (200, "[6765, 10946, 17711]"), "{b}");

            let (s, b) = call(&mut rt, "GET", "/count");
            assert_eq!((s, b.as_str()), (200, "notes: 0"), "{b}");
            let (s, b) = call(&mut rt, "POST", "/add");
            assert!(s < 300, "{s} {b}");
            // Not kept by the cache of purity: the task reads the database again.
            let (s, b) = call(&mut rt, "GET", "/count");
            assert_eq!((s, b.as_str()), (200, "notes: 1"), "{b}");
        })
        .unwrap()
        .join()
        .unwrap();
}
