//! `facile new NAME`: a whole application, ready to develop and to deploy.

use std::path::Path;

/// The files of a new application: (path, content); `{{nom}}` becomes its name.
const FILES: &[(&str, &str)] = &[
    ("app.ry", include_str!("../templates/app.ry")),
    ("client/compteur.ry", include_str!("../templates/client/compteur.ry")),
    ("public/style.css", include_str!("../templates/public/style.css")),
    ("README.md", include_str!("../templates/README.md")),
    (".gitignore", include_str!("../templates/gitignore")),
    ("Dockerfile", include_str!("../templates/Dockerfile")),
    ("heroku.yml", include_str!("../templates/heroku.yml")),
    ("app.json", include_str!("../templates/app.json")),
    (".github/workflows/ci.yml", include_str!("../templates/ci.yml")),
];

fn tr(en: &str, fr: &str) -> String {
    facile_core::util::tr(en, fr)
}

pub fn create(name: &str) -> Result<(), String> {
    let dir = Path::new(name);
    let shown = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| name.to_string());
    if shown.is_empty() || shown.contains(['"', '\\', '{', '}']) {
        return Err(tr("give a simple name: `facile new my-app`", "donnez un nom simple : `facile new mon-app`"));
    }
    if dir.exists() && dir.read_dir().map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(tr(&format!("`{name}` already exists and is not empty"), &format!("`{name}` existe déjà et n'est pas vide")));
    }
    for (path, content) in FILES {
        let file = dir.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&file, content.replace("{{nom}}", &shown)).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    println!(
        "{}",
        tr(
            &format!("✓ {shown} created.\n\n  cd {name}\n  facile dev        http://localhost:3000\n"),
            &format!("✓ {shown} est créée.\n\n  cd {name}\n  facile dev        http://localhost:3000\n"),
        )
    );
    Ok(())
}
