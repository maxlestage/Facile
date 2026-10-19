//! The files of Facile itself, served under `/_facile/`.

use facile_core::json::escaped;

const JS: &[u8] = include_bytes!("../assets/facile.js");
const CSS: &[u8] = include_bytes!("../assets/facile.css");
const ICON: &[u8] = include_bytes!("../assets/icon.svg");
/// The Rythme interpreter and the components' runtime, compiled to WebAssembly
/// (`scripts/build-wasm.sh` builds it from `crates/facile-client`).
const WASM: &[u8] = include_bytes!("../assets/facile.wasm");

/// `(name, content, Cache-Control)` of a file of Facile.
pub fn find(name: &str) -> Option<(&'static str, &'static [u8], &'static str)> {
    let short = "public, max-age=3600";
    Some(match name {
        "facile.js" => ("facile.js", JS, short),
        "facile.css" => ("facile.css", CSS, short),
        "icon.svg" => ("icon.svg", ICON, "public, max-age=86400"),
        "facile.wasm" => ("facile.wasm", WASM, short),
        _ => return None,
    })
}

fn shell(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{title}</title><link rel=\"stylesheet\" href=\"/_facile/facile.css\"><link rel=\"icon\" href=\"/_facile/icon.svg\"></head>\
<body><main class=\"facile-error\">{body}</main></body></html>"
    )
}

pub fn page_404(path: &str) -> String {
    shell(
        "404 · Introuvable",
        &format!(
            "<p class=\"facile-code\">404</p><h1>Cette page n'existe pas</h1><p><code>{}</code> ne correspond à aucune route.</p><p><a href=\"/\">Retour à l'accueil</a></p>",
            escaped(path)
        ),
    )
}

pub fn page_500(message: Option<&str>) -> String {
    match message {
        Some(m) => shell(
            "500 · Erreur",
            &format!(
                "<p class=\"facile-code\">500</p><h1>Le handler s'est arrêté sur une erreur</h1><pre class=\"facile-trace\">{}</pre><p>Cette page détaillée n'apparaît qu'en développement.</p>",
                escaped(m)
            ),
        ),
        None => shell("500 · Erreur", "<p class=\"facile-code\">500</p><h1>Une erreur est survenue</h1><p>Réessayez dans un instant.</p>"),
    }
}
