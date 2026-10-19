//! `facile`: create, check, develop and serve Facile applications.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use facile::runtime::{self, Global, Runtime};
use facile::server;

mod scaffold;

// Requests allocate many small values (records, texts): mimalloc keeps each
// thread's own memory close at hand.
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn tr(en: &str, fr: &str) -> String {
    facile_core::util::tr(en, fr)
}

fn usage() -> String {
    tr(
        "Facile: web applications written in Rythme.

Usage:
  facile new NAME          create an application (a folder NAME)
  facile dev [APP]         serve with live reload and detailed errors
  facile serve [APP]       serve for production (PORT, WEB_CONCURRENCY)
  facile check [APP]       check the application without serving it
  facile routes [APP]      list the routes
  facile version

APP is a folder holding app.ry, or a .ry file (default: the current folder).
Options: --port N, --workers N, --lang fr|en|es|de|pt|zh",
        "Facile : des applications web écrites en Rythme.

Utilisation :
  facile new NOM           crée une application (un dossier NOM)
  facile dev [APP]         sert avec rechargement et erreurs détaillées
  facile serve [APP]       sert pour la production (PORT, WEB_CONCURRENCY)
  facile check [APP]       vérifie l'application sans la servir
  facile routes [APP]      liste les routes
  facile version

APP est un dossier qui contient app.ry, ou un fichier .ry (par défaut : le dossier actuel).
Options : --port N, --workers N, --lang fr|en|es|de|pt|zh",
    )
}

struct Options {
    command: String,
    target: Option<String>,
    port: Option<u16>,
    workers: Option<usize>,
}

fn parse_args() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut o = Options { command: String::new(), target: None, port: None, workers: None };
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" | "-p" => {
                o.port =
                    Some(args.next().and_then(|p| p.parse().ok()).ok_or_else(|| tr("--port needs a number", "--port attend un nombre"))?)
            }
            "--workers" | "-w" => {
                o.workers = Some(
                    args.next().and_then(|p| p.parse().ok()).ok_or_else(|| tr("--workers needs a number", "--workers attend un nombre"))?,
                )
            }
            "--lang" => {
                let l = args.next().unwrap_or_default();
                match rythme::lang::parse(&l) {
                    Some(l) => rythme::lang::set(l),
                    None => return Err(tr(&format!("unknown language `{l}`"), &format!("langue inconnue `{l}`"))),
                }
            }
            "-h" | "--help" => o.command = "help".into(),
            "-V" | "--version" => o.command = "version".into(),
            _ if o.command.is_empty() => o.command = a,
            _ if o.target.is_none() => o.target = Some(a),
            _ => return Err(tr(&format!("unexpected argument `{a}`"), &format!("argument inattendu `{a}`"))),
        }
    }
    Ok(o)
}

/// The application file: `APP/app.ry`, or `APP` itself.
fn entry(target: Option<&str>) -> Result<PathBuf, String> {
    let t = PathBuf::from(target.unwrap_or("."));
    let file = if t.is_dir() { t.join("app.ry") } else { t };
    if !file.is_file() {
        return Err(tr(
            &format!("no application here: `{}` does not exist (create one with `facile new name`)", file.display()),
            &format!("pas d'application ici : `{}` n'existe pas (créez-en une avec `facile new nom`)", file.display()),
        ));
    }
    Ok(file.canonicalize().unwrap_or(file))
}

fn set_global(file: &Path, production: bool) {
    let root = file.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    // Paths written in the application (`read_file("docs/a.md")`, `sqlite://app.db`)
    // are read from its folder, wherever `facile` was started.
    let _ = std::env::set_current_dir(&root);
    runtime::set_global(Global { root, entry: file.to_path_buf(), production, lang: rythme::lang::code().to_string() });
}

/// Runs `f` on a thread with a large stack (Rythme recursion), like the workers.
fn on_big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(facile::STACK_SIZE)
        .spawn(move || {
            runtime::prepare_thread();
            f()
        })
        .expect("thread")
        .join()
        .expect("thread")
}

fn main() -> ExitCode {
    rythme::lang::set(rythme::lang::detect());
    let o = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("facile: {e}");
            return ExitCode::from(2);
        }
    };
    let result = match o.command.as_str() {
        "" | "help" => {
            println!("{}", usage());
            Ok(())
        }
        "version" => {
            println!("facile {} (Rythme {})", env!("CARGO_PKG_VERSION"), rythme::VERSION);
            Ok(())
        }
        "new" => match o.target.as_deref() {
            Some(name) => scaffold::create(name),
            None => {
                Err(tr("give the name of the application: `facile new my-app`", "donnez le nom de l'application : `facile new mon-app`"))
            }
        },
        "check" => check(o.target.as_deref()),
        "reference" => {
            print!("{}", reference());
            Ok(())
        }
        "routes" => routes(o.target.as_deref()),
        "serve" | "start" => serve(&o, std::env::var("FACILE_ENV").map(|e| e != "development").unwrap_or(true)),
        "run-dev" => serve(&o, false),
        "dev" => dev(&o),
        other => Err(tr(&format!("unknown command `{other}`\n\n{}", usage()), &format!("commande inconnue `{other}`\n\n{}", usage()))),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("facile: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Every function Facile adds to Rythme, as Markdown, in the language of the messages.
fn reference() -> String {
    use facile::host::{app, chain, mw, page, request, respond};
    use facile_core::{effects, evaluate, markdown, view};
    use rythme::builtins::Builtin;
    let fr = rythme::lang::code() == "fr";
    let mut out = String::new();
    out.push_str(if fr {
        "# Référence\n\nToutes les fonctions que Facile ajoute à Rythme, avec ce qu'elles font. Cette page est écrite par `facile reference --lang fr`, à partir du code.\n"
    } else {
        "# Reference\n\nEvery function Facile adds to Rythme, with what it does. This page is written by `facile reference --lang en`, from the code.\n"
    });
    let section = |out: &mut String, title: &str, list: &[&Builtin], prefix: &str| {
        out.push_str(&format!(
            "\n## {title}\n\n| {} | {} |\n|---|---|\n",
            if fr { "Fonction" } else { "Function" },
            if fr { "Effet" } else { "Effect" }
        ));
        for b in list {
            let sig = if prefix.is_empty() || b.sig.contains('.') { b.sig.to_string() } else { format!("{prefix}{}", b.sig) };
            let doc = if fr { b.fr } else { b.en };
            out.push_str(&format!("| `{sig}` | {} |\n", doc.replace('|', "\\|").replace('\n', " ")));
        }
    };
    let all = |l: &'static [Builtin]| l.iter().collect::<Vec<_>>();
    section(&mut out, "Application", &all(app::APP), "");
    section(&mut out, if fr { "Requête" } else { "Request" }, &request::ACCESSORS.iter().collect::<Vec<_>>(), "");
    section(&mut out, if fr { "Réponses" } else { "Responses" }, &all(respond::RESPOND), "");
    section(&mut out, "Middlewares", &[&chain::NEXT], "");
    section(&mut out, "mw", &all(mw::MW), "");
    section(
        &mut out,
        if fr { "Pages et vues" } else { "Pages and views" },
        &[all(page::PAGE), all(view::VIEW), all(markdown::MARKDOWN)].concat(),
        "",
    );
    section(
        &mut out,
        if fr { "Îlots : effets" } else { "Islands: effects" },
        &[all(effects::EFFECTS), all(evaluate::EVALUATE)].concat(),
        "",
    );
    section(&mut out, if fr { "Modèles et requêtes" } else { "Models and queries" }, &all(facile::orm::host::ORM), "");
    section(&mut out, "db", &all(facile::orm::host::DB), "");
    let tags: Vec<String> = view::TAGS.iter().map(|b| format!("`{}`", b.name)).collect();
    out.push_str(&format!(
        "\n## {}\n\n{}\n\n{}\n",
        if fr { "Éléments HTML" } else { "HTML elements" },
        if fr {
            "Chacun s'appelle `tag()`, `tag(enfants)`, `tag(attributs)` ou `tag(attributs, enfants)` ; `h(\"nom\", ...)` donne les autres."
        } else {
            "Each is called `tag()`, `tag(children)`, `tag(attributes)` or `tag(attributes, children)`; `h(\"name\", ...)` gives the others."
        },
        tags.join(", ")
    ));
    out
}

fn check(target: Option<&str>) -> Result<(), String> {
    let file = entry(target)?;
    set_global(&file, false);
    let f = file.clone();
    let loaded = on_big_stack(move || Runtime::load(&f, false).map(|r| (r.name.clone(), r.routes.len())).map_err(|e| e.0));
    let (name, n) = loaded?;
    println!("{}", tr(&format!("✓ {name}: no problems found, {n} route(s)"), &format!("✓ {name} : aucun problème trouvé, {n} route(s)")));
    Ok(())
}

fn routes(target: Option<&str>) -> Result<(), String> {
    let file = entry(target)?;
    set_global(&file, false);
    let f = file.clone();
    let routes = on_big_stack(move || Runtime::load(&f, false).map(|r| r.routes.clone()).map_err(|e| e.0))?;
    for (m, p) in routes {
        println!("{m:<7} {p}");
    }
    Ok(())
}

fn serve(o: &Options, production: bool) -> Result<(), String> {
    let file = entry(o.target.as_deref())?;
    set_global(&file, production);
    // Loaded once here to show its problems once, and read its settings.
    let f = file.clone();
    let settings = on_big_stack(move || {
        Runtime::load(&f, false).map(|mut r| {
            if let Some(l) = &r.settings.lang {
                runtime::set_app_lang(l);
            }
            // The tables are made, and `on_start` runs, once, before the workers start.
            let started = r.start(true).err();
            (r.settings.workers, r.settings.body_limit, r.name.clone(), started)
        })
    })
    .map_err(|e| e.0)?;
    let (workers, body_limit, name, db_error) = settings;
    if let Some(e) = db_error {
        return Err(e);
    }
    let port = o.port.or_else(|| std::env::var("PORT").ok().and_then(|p| p.parse().ok())).unwrap_or(3000);
    let mut config = server::Config { addr: SocketAddr::from(([0, 0, 0, 0], port)), ..server::Config::default() };
    if let Some(w) = o.workers.or_else(|| std::env::var("WEB_CONCURRENCY").ok().and_then(|w| w.parse().ok())).or(workers) {
        config.workers = w.max(1);
    }
    if let Some(b) = body_limit {
        config.max_body = b;
    }
    let mode = if production { "production".to_string() } else { tr("development", "développement") };
    println!(
        "{}",
        tr(
            &format!("Facile · {name} · http://localhost:{port} · {} workers · {mode}", config.workers),
            &format!("Facile · {name} · http://localhost:{port} · {} workers · {mode}", config.workers),
        )
    );
    server::serve(config, move |w| {
        let mut rt = match Runtime::load(&file, true) {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("facile: {}", e.0);
                std::process::exit(1);
            }
        };
        let _ = w;
        if let Err(e) = rt.start(false) {
            eprintln!("facile: {e}");
            std::process::exit(1);
        }
        Box::new(rt)
    })
    .map_err(|e| tr(&format!("cannot listen on port {port}: {e}"), &format!("impossible d'écouter sur le port {port} : {e}")))
}

/// The newest change among the application's files.
fn last_change(root: &Path) -> SystemTime {
    let mut newest = SystemTime::UNIX_EPOCH;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if name.ends_with(".ry") {
                if let Ok(t) = e.metadata().and_then(|m| m.modified()) {
                    newest = newest.max(t);
                }
            }
        }
    }
    newest
}

/// Serves in a child process, started again when a `.ry` file changes.
fn dev(o: &Options) -> Result<(), String> {
    let file = entry(o.target.as_deref())?;
    let root = file.parent().map(Path::to_path_buf).unwrap_or_default();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    loop {
        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("run-dev").arg(&file).env("FACILE_ENV", "development");
        if let Some(p) = o.port {
            cmd.arg("--port").arg(p.to_string());
        }
        if let Some(w) = o.workers {
            cmd.arg("--workers").arg(w.to_string());
        }
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let stamp = last_change(&root);
        loop {
            std::thread::sleep(Duration::from_millis(300));
            if let Ok(Some(_)) = child.try_wait() {
                // Stopped (an error in the application): wait for a change.
                while last_change(&root) == stamp {
                    std::thread::sleep(Duration::from_millis(300));
                }
                break;
            }
            if last_change(&root) != stamp {
                let _ = child.kill();
                let _ = child.wait();
                println!("{}", tr("↻ a file changed: reloading", "↻ un fichier a changé : rechargement"));
                break;
            }
        }
    }
}
