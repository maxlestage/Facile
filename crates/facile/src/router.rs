//! Routes: `/`, `/taches/:id`, `/fichiers/*chemin`.
//!
//! Paths without parameters are found in a hash table; the others are
//! compared segment by segment, in the order they were declared.

use rustc_hash::FxHashMap as HashMap;
use std::rc::Rc;

use rythme::value::{Record, Shape, Value};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Segment {
    Fixed(String),
    Param(String),
    /// `*name`: the rest of the path, slashes included.
    Rest(String),
}

#[derive(Clone, Debug)]
pub struct Pattern {
    pub text: String,
    pub segments: Vec<Segment>,
}

impl Pattern {
    pub fn parse(path: &str) -> Result<Pattern, String> {
        if !path.starts_with('/') {
            return Err(format!("a route path starts with `/`: `{path}`"));
        }
        let mut segments = Vec::new();
        let parts: Vec<&str> = path.split('/').skip(1).collect();
        for (i, part) in parts.iter().enumerate() {
            if let Some(name) = part.strip_prefix(':') {
                if name.is_empty() {
                    return Err(format!("a parameter needs a name after `:` in `{path}`"));
                }
                segments.push(Segment::Param(name.to_string()));
            } else if let Some(name) = part.strip_prefix('*') {
                if i + 1 != parts.len() {
                    return Err(format!("`*{name}` takes the rest of the path: it must come last in `{path}`"));
                }
                segments.push(Segment::Rest(if name.is_empty() { "rest".into() } else { name.to_string() }));
            } else if !(part.is_empty() && i + 1 == parts.len() && i > 0) {
                segments.push(Segment::Fixed(part.to_string()));
            }
        }
        Ok(Pattern { text: path.to_string(), segments })
    }

    pub fn is_static(&self) -> bool {
        self.segments.iter().all(|s| matches!(s, Segment::Fixed(_)))
    }

    pub fn param_names(&self) -> Vec<Rc<str>> {
        self.segments
            .iter()
            .filter_map(|s| match s {
                Segment::Param(n) | Segment::Rest(n) => Some(Rc::from(n.as_str())),
                Segment::Fixed(_) => None,
            })
            .collect()
    }

    /// The parameters' values when `path` matches.
    fn matches<'p>(&self, path: &'p str, out: &mut Vec<&'p str>) -> bool {
        out.clear();
        let mut rest = path.strip_prefix('/').unwrap_or(path);
        for (i, seg) in self.segments.iter().enumerate() {
            if let Segment::Rest(_) = seg {
                out.push(rest);
                return true;
            }
            let (part, after) = match rest.find('/') {
                Some(j) => (&rest[..j], &rest[j + 1..]),
                None => (rest, ""),
            };
            let last = i + 1 == self.segments.len();
            if part.is_empty() && !(matches!(seg, Segment::Fixed(f) if f.is_empty())) {
                return false;
            }
            match seg {
                Segment::Fixed(f) => {
                    if f != part {
                        return false;
                    }
                }
                Segment::Param(_) => out.push(part),
                Segment::Rest(_) => unreachable!(),
            }
            rest = after;
            if last {
                // `/a/b/` matches `/a/b`.
                return rest.is_empty();
            }
            if rest.is_empty() && !path.ends_with('/') {
                return false;
            }
        }
        rest.is_empty()
    }
}

pub struct Route<T> {
    pub method: String,
    pub pattern: Pattern,
    /// The shape of the `params` record, made once.
    pub params: Rc<Shape>,
    pub target: T,
}

pub struct Router<T> {
    routes: Vec<Route<T>>,
    /// Paths without parameters: their methods and routes.
    fixed: HashMap<String, Vec<(String, usize)>>,
    dynamic: Vec<usize>,
}

pub struct Found<'r, 'p, T> {
    pub route: &'r Route<T>,
    pub index: usize,
    pub values: Vec<&'p str>,
}

impl<T> Default for Router<T> {
    fn default() -> Self {
        Router { routes: Vec::new(), fixed: HashMap::default(), dynamic: Vec::new() }
    }
}

impl<T> Router<T> {
    pub fn add(&mut self, method: &str, pattern: Pattern, target: T) {
        let i = self.routes.len();
        let method = method.to_ascii_uppercase();
        if pattern.is_static() {
            let list = self.fixed.entry(normalize(&pattern.text).to_string()).or_default();
            if !list.iter().any(|(m, _)| *m == method) {
                list.push((method.clone(), i));
            }
        } else {
            self.dynamic.push(i);
        }
        let params = Rc::new(Shape::new(pattern.param_names()));
        self.routes.push(Route { method, pattern, params, target });
    }

    pub fn routes(&self) -> &[Route<T>] {
        &self.routes
    }

    /// The route for this method and path. `HEAD` uses the `GET` routes.
    pub fn find<'r, 'p>(&'r self, method: &str, path: &'p str) -> Option<Found<'r, 'p, T>> {
        let fixed = self.fixed.get(normalize(path));
        let lookup = |m: &str| -> Option<Found<'r, 'p, T>> {
            if let Some(&(_, i)) = fixed.and_then(|l| l.iter().find(|(x, _)| x == m)) {
                return Some(Found { route: &self.routes[i], index: i, values: Vec::new() });
            }
            let mut values = Vec::new();
            for &i in &self.dynamic {
                let r = &self.routes[i];
                if (r.method == m || r.method == "ANY") && r.pattern.matches(path, &mut values) {
                    return Some(Found { route: r, index: i, values });
                }
            }
            if let Some(&(_, i)) = fixed.and_then(|l| l.iter().find(|(x, _)| x == "ANY")) {
                return Some(Found { route: &self.routes[i], index: i, values: Vec::new() });
            }
            None
        };
        lookup(method).or_else(|| if method == "HEAD" { lookup("GET") } else { None })
    }

    /// The methods that have a route for this path (for `405` and `Allow`).
    pub fn methods_for(&self, path: &str) -> Vec<String> {
        let mut values = Vec::new();
        let mut out: Vec<String> = self
            .routes
            .iter()
            .filter(|r| {
                if r.pattern.is_static() {
                    normalize(&r.pattern.text) == normalize(path)
                } else {
                    r.pattern.matches(path, &mut values)
                }
            })
            .map(|r| r.method.clone())
            .collect();
        out.dedup();
        out
    }
}

/// `/a/b/` and `/a/b` are the same route.
fn normalize(path: &str) -> &str {
    if path.len() > 1 && path.ends_with('/') {
        let p = path.trim_end_matches('/');
        if p.is_empty() {
            "/"
        } else {
            p
        }
    } else {
        path
    }
}

/// The `params` record of a found route.
pub fn params_record<T>(found: &Found<'_, '_, T>) -> Value {
    let vals = found.values.iter().map(|v| Value::text(&crate::http::decode(v))).collect();
    Value::Record(Rc::new(Record::with_shape(found.route.params.clone(), vals)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router() -> Router<u8> {
        let mut r = Router::default();
        r.add("GET", Pattern::parse("/").unwrap(), 0);
        r.add("GET", Pattern::parse("/taches").unwrap(), 1);
        r.add("GET", Pattern::parse("/taches/:id").unwrap(), 2);
        r.add("POST", Pattern::parse("/taches").unwrap(), 3);
        r.add("GET", Pattern::parse("/fichiers/*chemin").unwrap(), 4);
        r.add("GET", Pattern::parse("/u/:a/p/:b").unwrap(), 5);
        r
    }

    #[test]
    fn finds_routes() {
        let r = router();
        assert_eq!(r.find("GET", "/").unwrap().route.target, 0);
        assert_eq!(r.find("GET", "/taches").unwrap().route.target, 1);
        assert_eq!(r.find("GET", "/taches/").unwrap().route.target, 1);
        assert_eq!(r.find("HEAD", "/taches").unwrap().route.target, 1);
        assert_eq!(r.find("POST", "/taches").unwrap().route.target, 3);
        let f = r.find("GET", "/taches/42").unwrap();
        assert_eq!((f.route.target, f.values.clone()), (2, vec!["42"]));
        let f = r.find("GET", "/fichiers/a/b.txt").unwrap();
        assert_eq!((f.route.target, f.values.clone()), (4, vec!["a/b.txt"]));
        let f = r.find("GET", "/u/1/p/2").unwrap();
        assert_eq!(f.values, vec!["1", "2"]);
        assert!(r.find("GET", "/taches/42/x").is_none());
        assert!(r.find("GET", "/u/1/p").is_none());
        assert!(r.find("DELETE", "/taches").is_none());
        assert_eq!(r.methods_for("/taches"), vec!["GET", "POST"]);
    }
}
