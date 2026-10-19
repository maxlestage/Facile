//! Applications answering requests, without the network: routes, pages,
//! middlewares, the cache of pure responses, islands, static files.

use std::path::{Path, PathBuf};
use std::sync::Once;

use facile::runtime::{self, Global, Runtime};
use facile::testing::{parse_response, request};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/apps")
}

fn global() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        runtime::set_global(Global { root: root(), entry: root().join("site/app.ry"), production: false, lang: "en".into() });
    });
}

/// Writes `files` in a new folder, loads its `app.ry`, and runs `f` with it.
fn with_app(name: &str, files: &[(&str, &str)], f: impl FnOnce(&mut Runtime) + Send + 'static) {
    global();
    let dir = std::env::temp_dir().join(format!("facile-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, content) in files {
        let p = dir.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    let entry = dir.join("app.ry");
    std::thread::Builder::new()
        .stack_size(facile::STACK_SIZE)
        .spawn(move || {
            runtime::prepare_thread();
            let mut rt = Runtime::load(&entry, false).expect("the application loads");
            rt.start(true).expect("it starts");
            f(&mut rt);
        })
        .unwrap()
        .join()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Answer {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

fn send(rt: &mut Runtime, method: &str, target: &str, headers: &[(&str, &str)], body: &str) -> Answer {
    let (status, headers, body) = parse_response(&rt.respond(&request(method, target, headers, body)));
    Answer { status, headers, body }
}

fn get(rt: &mut Runtime, target: &str) -> Answer {
    send(rt, "GET", target, &[], "")
}

const BASIC: &str = r#"
fn accueil(req) = page({title: "Accueil", description: "Une page.", lang: "fr", url: "https://a.fr/"}, [h1("Bonjour"), p(["<script>"])])
fn bonjour(req) = "Bonjour, {req.params.nom} !"
fn fichier(req) = req.params.chemin
fn info(req) = {path: req.path, query: req.query, agent: req.header("user-agent")}
fn creer(req) = match req.json()
  Ok(v) -> json(v, 201)
  Err(e) -> problem(400, e)
end
fn heure(req) = text(now().second >= 0)
fn absent(req) = None
fn interdit(req) = Err(:forbidden)
fn invalide(req) = Err({status: 422, champ: "titre"})
fn vide(req) = ()
fn redirige(req) = redirect("/ailleurs")
fn cookie(req) = "ok" |> with_cookie("theme", "sombre", {max_age: 60}) |> with_header("x-test", "oui")
fn boom(req) = 1 // 0

app("Base")
  |> on_get("/", accueil)
  |> on_get("/bonjour/:nom", bonjour)
  |> on_get("/fichiers/*chemin", fichier)
  |> on_get("/info", info)
  |> on_post("/creer", creer)
  |> on_get("/heure", heure)
  |> on_get("/absent", absent)
  |> on_get("/interdit", interdit)
  |> on_get("/invalide", invalide)
  |> on_get("/vide", vide)
  |> on_get("/redirige", redirige)
  |> on_get("/cookie", cookie)
  |> on_get("/boom", boom)
"#;

#[test]
fn routes_requests_and_responses() {
    with_app("base", &[("app.ry", BASIC)], |rt| {
        let a = get(rt, "/");
        assert_eq!(a.status, 200);
        assert!(a.body.starts_with("<!doctype html>"), "{}", a.body);
        for part in [
            "<html lang=\"fr\">",
            "<title>Accueil</title>",
            "<meta name=\"description\" content=\"Une page.\">",
            "<link rel=\"canonical\" href=\"https://a.fr/\">",
            "og:title",
            "twitter:card",
            "<h1>Bonjour</h1>",
            "&lt;script&gt;",
        ] {
            assert!(a.body.contains(part), "{part} in {}", a.body);
        }
        assert_eq!(get(rt, "/bonjour/Ada").body, "Bonjour, Ada !");
        assert_eq!(get(rt, "/bonjour/Ada%20L").body, "Bonjour, Ada L !");
        assert_eq!(get(rt, "/fichiers/a/b.txt").body, "a/b.txt");
        let a = send(rt, "GET", "/info?x=1&y=deux", &[("user-agent", "test")], "");
        assert_eq!(a.body, r#"{"path":"/info","query":{"x":"1","y":"deux"},"agent":"test"}"#);
        assert_eq!(a.header("content-type"), Some("application/json"));
        let a = send(rt, "POST", "/creer", &[("content-type", "application/json")], r#"{"a":[1,2.5,null]}"#);
        assert_eq!((a.status, a.body.as_str()), (201, r#"{"a":[1,2.5,null]}"#));
        assert_eq!(send(rt, "POST", "/creer", &[], "{").status, 400);
        assert_eq!(get(rt, "/absent").status, 404);
        assert_eq!(get(rt, "/interdit").status, 403);
        let a = get(rt, "/invalide");
        assert_eq!((a.status, a.body.as_str()), (422, r#"{"status":422,"champ":"titre"}"#));
        assert_eq!(get(rt, "/vide").status, 204);
        let a = get(rt, "/redirige");
        assert_eq!((a.status, a.header("location")), (303, Some("/ailleurs")));
        let a = get(rt, "/cookie");
        assert_eq!(a.header("x-test"), Some("oui"));
        assert!(
            a.header("set-cookie").is_some_and(|c| c.starts_with("theme=sombre; Path=/; Max-Age=60; HttpOnly; SameSite=Lax")),
            "{:?}",
            a.headers
        );
        // Unknown paths, wrong methods, HEAD.
        assert_eq!(get(rt, "/nulle-part").status, 404);
        let a = send(rt, "DELETE", "/bonjour/Ada", &[], "");
        assert_eq!((a.status, a.header("allow")), (405, Some("GET")));
        let a = send(rt, "HEAD", "/bonjour/Ada", &[], "");
        assert_eq!((a.status, a.body.as_str(), a.header("content-length")), (200, "", Some("14")));
        // A failing handler: the message of Rythme during development.
        let a = get(rt, "/boom");
        assert_eq!(a.status, 500);
        assert!(a.body.contains("E008"), "{}", a.body);
    });
}

#[test]
fn pure_responses_are_kept_and_impure_ones_are_not() {
    const APP: &str = r#"
fn pure(req) = "pure {req.params.n}"
fn lit_en_tete(req) = "agent {unwrap_or(req.header("user-agent"), "?")}"
fn horloge(req) = text(time())
app("Cache") |> on_get("/pure/:n", pure) |> on_get("/agent", lit_en_tete) |> on_get("/horloge", horloge)
"#;
    with_app("cache", &[("app.ry", APP)], |rt| {
        assert_eq!(get(rt, "/pure/1").body, "pure 1");
        assert_eq!(get(rt, "/pure/1").body, "pure 1");
        assert_eq!(get(rt, "/pure/2").body, "pure 2");
        // A header was read: the response is not given to another client.
        assert_eq!(send(rt, "GET", "/agent", &[("user-agent", "a")], "").body, "agent a");
        assert_eq!(send(rt, "GET", "/agent", &[("user-agent", "b")], "").body, "agent b");
        // The clock was read: computed each time.
        let t1 = get(rt, "/horloge").body;
        std::thread::sleep(std::time::Duration::from_millis(20));
        let t2 = get(rt, "/horloge").body;
        assert_ne!(t1, t2);
    });
}

#[test]
fn middlewares_in_rythme_and_in_rust() {
    const APP: &str = r#"
fn journal(req, next)
  res = next(req)
  with_header(res, "x-journal", "{req.path} {res.status}")
end
fn garde(req, next) = if req.query == {secret: "oui"} then next({..req, invite: "Ada"}) else problem(401, "non")
fn bonjour(req) = "Bonjour {unwrap_or(get(req, "invite"), "?")}"
fn ecrire(req) = "écrit"
prive = app("Privé") |> on_get("/", bonjour) |> middleware(garde)
app("MW")
  |> on_get("/", bonjour)
  |> on_post("/ecrire", ecrire)
  |> mount("/prive", prive)
  |> middleware(journal)
  |> middleware(mw.shield())
  |> middleware(mw.cors(["https://ami.fr"]))
  |> middleware(mw.timing())
"#;
    with_app("mw", &[("app.ry", APP)], |rt| {
        let a = get(rt, "/");
        assert_eq!((a.status, a.body.as_str()), (200, "Bonjour ?"));
        assert_eq!(a.header("x-journal"), Some("/ 200"));
        assert!(a.header("content-security-policy").is_some_and(|c| c.contains("wasm-unsafe-eval")));
        assert_eq!(a.header("x-frame-options"), Some("SAMEORIGIN"));
        assert!(a.header("x-request-id").is_some_and(|i| i.len() == 16));
        assert!(a.header("server-timing").is_some());
        // A mounted application with its own middleware.
        assert_eq!(get(rt, "/prive").status, 401);
        assert_eq!(get(rt, "/prive?secret=oui").body, "Bonjour Ada");
        // WAF and robots.
        assert_eq!(get(rt, "/?q=1%27%20UNION%20SELECT%20motdepasse").status, 403);
        assert_eq!(get(rt, "/?x=%3Cscript%3Ealert(1)%3C/script%3E").status, 403);
        assert_eq!(get(rt, "/static/..%2f..%2fetc/passwd").status, 403);
        assert_eq!(get(rt, "/.env").status, 404);
        assert_eq!(send(rt, "GET", "/", &[("user-agent", "sqlmap/1.7")], "").status, 404);
        // CSRF: a form sent by another site.
        assert_eq!(send(rt, "POST", "/ecrire", &[("sec-fetch-site", "cross-site")], "").status, 403);
        assert_eq!(send(rt, "POST", "/ecrire", &[("origin", "https://pirate.fr"), ("host", "localhost")], "").status, 403);
        assert_eq!(send(rt, "POST", "/ecrire", &[("sec-fetch-site", "same-origin")], "").status, 200);
        // CORS, with a preflight request.
        let a = send(rt, "OPTIONS", "/ecrire", &[("origin", "https://ami.fr"), ("access-control-request-method", "POST")], "");
        assert_eq!((a.status, a.header("access-control-allow-origin")), (204, Some("https://ami.fr")));
        let a = send(rt, "GET", "/", &[("origin", "https://ami.fr")], "");
        assert_eq!(a.header("access-control-allow-origin"), Some("https://ami.fr"));
        let a = send(rt, "GET", "/", &[("origin", "https://autre.fr")], "");
        assert_eq!(a.header("access-control-allow-origin"), None);
        // Compression of a large response.
        let a = send(rt, "GET", "/_facile/facile.js", &[("accept-encoding", "gzip, br")], "");
        assert_eq!(a.header("content-encoding"), Some("gzip"));
    });
}

#[test]
fn rate_limits_and_authentication() {
    const APP: &str = r#"
fn ok(req) = "ok"
admin = app("Admin") |> on_get("/", ok) |> middleware(mw.basic_auth("ada", "lovelace"))
api = app("API") |> on_get("/", ok) |> middleware(mw.bearer("0123456789abcdef0123"))
app("Limites")
  |> on_get("/", ok)
  |> mount("/admin", admin)
  |> mount("/api", api)
  |> middleware(mw.rate_limit(3))
"#;
    with_app("limits", &[("app.ry", APP)], |rt| {
        let ip = [("x-forwarded-for", "203.0.113.9")];
        for _ in 0..3 {
            assert_eq!(send(rt, "GET", "/", &ip, "").status, 200);
        }
        let a = send(rt, "GET", "/", &ip, "");
        assert_eq!(a.status, 429);
        assert!(a.header("retry-after").is_some());
        assert_eq!(send(rt, "GET", "/", &[("x-forwarded-for", "203.0.113.10")], "").status, 200);
        let other = |n: u8| format!("198.51.100.{n}");
        assert_eq!(send(rt, "GET", "/admin", &[("x-forwarded-for", &other(1))], "").status, 401);
        // ada:lovelace
        assert_eq!(
            send(rt, "GET", "/admin", &[("x-forwarded-for", &other(2)), ("authorization", "Basic YWRhOmxvdmVsYWNl")], "").status,
            200
        );
        assert_eq!(send(rt, "GET", "/api", &[("x-forwarded-for", &other(3)), ("authorization", "Bearer faux")], "").status, 401);
        assert_eq!(
            send(rt, "GET", "/api", &[("x-forwarded-for", &other(4)), ("authorization", "Bearer 0123456789abcdef0123")], "").status,
            200
        );
    });
}

const COMPTEUR: &str = r#"
fn init(props) = {n: props.depart}
fn update(etat, message) = match message
  :plus -> {..etat, n: etat.n + 1}
end
fn view(etat) = div({class: "compteur"}, [button({on_click: :plus}, ["+"]), strong([etat.n])])
component = {name: "compteur", init, update, view}
"#;

#[test]
fn islands_static_files_and_markdown() {
    const APP: &str = r#"
use client/compteur
guide = unwrap_or(read_file("guide.md"), "")
fn accueil(req) = page({title: "Îlot"}, [island(compteur.component, {depart: 41}), markdown(guide)])
app("Îlots") |> on_get("/", accueil) |> assets("/static", "public")
"#;
    with_app(
        "islands",
        &[
            ("app.ry", APP),
            ("client/compteur.ry", COMPTEUR),
            ("public/style.css", "body { color: red; }"),
            ("guide.md", "## Le guide\n\n```rythme\nfn f(x) = x\n```"),
        ],
        |rt| {
            let a = get(rt, "/");
            assert!(a.body.contains("<facile-island data-component=\"compteur\" data-props=\"{&quot;depart&quot;:41}\" data-src=\"/_facile/client/compteur.ry\"><div class=\"compteur\"><button>+</button><strong>41</strong></div></facile-island>"), "{}", a.body);
            assert!(a.body.contains("<script type=\"module\" src=\"/_facile/facile.js\"></script>"));
            assert!(a.body.contains("<h2 id=\"le-guide\">"), "{}", a.body);
            assert!(a.body.contains("<span class=\"tok-keyword\">fn</span>"));
            assert_eq!(get(rt, "/_facile/client/compteur.ry").body, COMPTEUR);
            let a = get(rt, "/_facile/facile.wasm");
            assert_eq!((a.status, a.header("content-type")), (200, Some("application/wasm")));
            let a = get(rt, "/static/style.css");
            assert_eq!((a.status, a.body.as_str()), (200, "body { color: red; }"));
            let etag = a.header("etag").unwrap().to_string();
            assert_eq!(send(rt, "GET", "/static/style.css", &[("if-none-match", &etag)], "").status, 304);
            assert_eq!(get(rt, "/static/../app.ry").status, 404);
        },
    );
}

#[test]
fn the_new_application_and_the_examples_load() {
    global();
    let dir = std::env::temp_dir().join(format!("facile-new-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_facile")).arg("new").arg(&dir).status().unwrap();
    assert!(status.success());
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for app in [dir.clone(), repo.join("examples/taches"), repo.join("site"), repo.join("bench/app.ry")] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_facile")).arg("check").arg(&app).output().unwrap();
        assert!(
            out.status.success(),
            "{}: {}{}",
            app.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
