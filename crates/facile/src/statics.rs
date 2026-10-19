//! Files served as they are: `assets(app, "/static", "public")`, and the
//! files of Facile itself under `/_facile/`.
//!
//! In production, each file is read once, with its gzip version and its
//! ETag; during development it is read again at each request (edit, reload).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use sha2::{Digest, Sha256};

use crate::http::{compressible, content_type, gzip};

pub struct Asset {
    pub body: Rc<[u8]>,
    pub gz: Option<Rc<[u8]>>,
    pub etag: String,
    pub content_type: &'static str,
    pub cache_control: &'static str,
}

impl Asset {
    pub fn new(path: &str, bytes: Vec<u8>, cache_control: &'static str) -> Asset {
        let ct = content_type(path);
        let digest = Sha256::digest(&bytes);
        let etag = format!("\"{}\"", digest.iter().take(10).map(|b| format!("{b:02x}")).collect::<String>());
        let gz = (compressible(ct) && bytes.len() > 1024).then(|| gzip(&bytes)).filter(|g| g.len() < bytes.len()).map(Rc::from);
        Asset { body: Rc::from(bytes), gz, etag, content_type: ct, cache_control }
    }
}

/// A folder served under a prefix.
struct Mount {
    prefix: String,
    folder: PathBuf,
}

#[derive(Default)]
pub struct Statics {
    mounts: Vec<Mount>,
    files: HashMap<String, Rc<Asset>>,
    dev: bool,
}

/// `app.4f3a9c1e.js`: a name that changes with the content can be kept for a year.
fn fingerprinted(name: &str) -> bool {
    name.split('.').any(|part| part.len() >= 8 && part.chars().all(|c| c.is_ascii_hexdigit()))
}

fn cache_for(name: &str, dev: bool) -> &'static str {
    if dev {
        "no-cache"
    } else if fingerprinted(name) {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    }
}

impl Statics {
    pub fn new(dev: bool) -> Statics {
        Statics { dev, ..Statics::default() }
    }

    pub fn mount(&mut self, prefix: &str, folder: PathBuf) {
        let prefix = prefix.trim_end_matches('/').to_string();
        if !self.dev {
            let mut stack = vec![folder.clone()];
            while let Some(dir) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&dir) else { continue };
                for e in entries.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if let Ok(rel) = p.strip_prefix(&folder) {
                        let url = format!("{prefix}/{}", rel.to_string_lossy().replace('\\', "/"));
                        if let Ok(bytes) = std::fs::read(&p) {
                            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            self.files.insert(url, Rc::new(Asset::new(&name, bytes, cache_for(&name, false))));
                        }
                    }
                }
            }
        }
        self.mounts.push(Mount { prefix, folder });
    }

    pub fn is_empty(&self) -> bool {
        self.mounts.is_empty()
    }

    /// The file for this path, if it is one of the served files.
    pub fn find(&self, path: &str) -> Option<Rc<Asset>> {
        let path = if path.ends_with('/') { format!("{path}index.html") } else { path.to_string() };
        if !self.dev {
            return self.files.get(&path).cloned();
        }
        for m in &self.mounts {
            let Some(rest) = path.strip_prefix(&m.prefix) else { continue };
            let Some(rest) = rest.strip_prefix('/') else { continue };
            if let Some(file) = safe_join(&m.folder, rest) {
                if let Ok(bytes) = std::fs::read(&file) {
                    return Some(Rc::new(Asset::new(rest, bytes, cache_for(rest, true))));
                }
            }
        }
        None
    }
}

/// `folder/rest`, unless `rest` tries to leave the folder.
pub fn safe_join(folder: &Path, rest: &str) -> Option<PathBuf> {
    let decoded = crate::http::percent_decode(rest);
    let mut p = folder.to_path_buf();
    for part in decoded.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return None,
            part if part.contains('\0') || part.contains(':') => return None,
            part => p.push(part),
        }
    }
    p.is_file().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(fingerprinted("app.4f3a9c1e.js"));
        assert!(!fingerprinted("site.css"));
        assert!(safe_join(Path::new("."), "../etc/passwd").is_none());
        assert!(safe_join(Path::new("."), "%2e%2e/x").is_none());
    }
}
