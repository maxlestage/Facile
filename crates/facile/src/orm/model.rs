//! Models: `taches = model("taches", {titre: field(:text, {required: true}), fait: :bool})`.

use std::rc::Rc;

use facile_core::util::{field, name_of, tr};
use rythme::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Int,
    Float,
    Bool,
    /// `2026-10-19`, kept as text.
    Date,
    /// `2026-10-19T08:30:00Z`, kept as text.
    DateTime,
    /// Any value, kept as JSON.
    Json,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "text" | "string" => Kind::Text,
            "int" | "integer" => Kind::Int,
            "float" | "number" => Kind::Float,
            "bool" | "boolean" => Kind::Bool,
            "date" => Kind::Date,
            "datetime" | "timestamp" => Kind::DateTime,
            "json" => Kind::Json,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Int => "int",
            Kind::Float => "float",
            Kind::Bool => "bool",
            Kind::Date => "date",
            Kind::DateTime => "datetime",
            Kind::Json => "json",
        }
    }
}

#[derive(Clone)]
pub struct Field {
    /// The name in records, and the column (`projet_id` for `projet: belongs_to(...)`).
    pub name: String,
    pub kind: Kind,
    pub required: bool,
    pub unique: bool,
    pub index: bool,
    pub default: Option<Value>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub email: bool,
    pub one_of: Vec<Value>,
    /// `belongs_to("projets")`: the table referred to, and the relation's name (`projet`).
    pub refers: Option<(String, String)>,
}

/// `projets` has many `taches` through their field `projet_id`.
#[derive(Clone, Debug)]
pub struct HasMany {
    pub name: String,
    pub table: String,
    pub foreign: String,
}

#[derive(Clone)]
pub struct Model {
    pub table: String,
    pub fields: Vec<Field>,
    pub has_many: Vec<HasMany>,
    pub timestamps: bool,
}

/// A name of table or column: it goes in SQL as it is, so it is checked.
pub fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 63
        && s.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl Model {
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// The columns: `id`, the fields, then the timestamps.
    pub fn columns(&self) -> Vec<String> {
        let mut c = vec!["id".to_string()];
        c.extend(self.fields.iter().map(|f| f.name.clone()));
        if self.timestamps {
            c.push("created_at".into());
            c.push("updated_at".into());
        }
        c
    }

    /// The kind of a column (`id` and the timestamps included).
    pub fn kind_of(&self, column: &str) -> Option<Kind> {
        match column {
            "id" => Some(Kind::Int),
            "created_at" | "updated_at" if self.timestamps => Some(Kind::DateTime),
            _ => self.field(column).map(|f| f.kind),
        }
    }

    pub fn from_value(v: &Value) -> Result<Model, String> {
        if field(v, "facile").and_then(name_of).as_deref() != Some("model") {
            return Err(tr("this is not a model made by `model(...)`", "ce n'est pas un modèle fait par `model(...)`"));
        }
        let table = field(v, "table").map(|t| t.display()).unwrap_or_default();
        let mut fields = Vec::new();
        let mut has_many = Vec::new();
        if let Some(Value::List(l)) = field(v, "fields") {
            for f in l.iter() {
                let name = field(f, "name").map(|n| n.display()).unwrap_or_default();
                if let Some(t) = field(f, "has_many") {
                    has_many.push(HasMany {
                        name,
                        table: t.display(),
                        foreign: field(f, "foreign").map(|x| x.display()).unwrap_or_default(),
                    });
                    continue;
                }
                let kind = field(f, "kind").and_then(name_of).and_then(|k| Kind::parse(&k)).unwrap_or(Kind::Text);
                let num = |k: &str| match field(f, k) {
                    Some(Value::Int(n)) => Some(*n as f64),
                    Some(Value::Float(x)) => Some(*x),
                    _ => None,
                };
                let flag = |k: &str| matches!(field(f, k), Some(Value::Bool(true)));
                let refers = field(f, "refers").map(|t| (t.display(), field(f, "relation").map(|r| r.display()).unwrap_or_default()));
                fields.push(Field {
                    name,
                    kind,
                    required: flag("required"),
                    unique: flag("unique"),
                    index: flag("index") || refers.is_some(),
                    default: field(f, "default").filter(|d| !matches!(d, Value::Unit)).cloned(),
                    min: num("min"),
                    max: num("max"),
                    email: flag("email"),
                    one_of: match field(f, "one_of") {
                        Some(Value::List(l)) => l.iter().cloned().collect(),
                        _ => Vec::new(),
                    },
                    refers,
                });
            }
        }
        Ok(Model { table, fields, has_many, timestamps: !matches!(field(v, "timestamps"), Some(Value::Bool(false))) })
    }
}

thread_local! {
    /// Models already read, by table: reading a model value again is avoided.
    static MODELS: std::cell::RefCell<std::collections::HashMap<String, Rc<Model>>> = Default::default();
}

/// The model of a model value (read once per table).
pub fn model_of(v: &Value) -> Result<Rc<Model>, String> {
    let table = field(v, "table").map(|t| t.display()).unwrap_or_default();
    if let Some(m) = MODELS.with(|m| m.borrow().get(&table).cloned()) {
        return Ok(m);
    }
    let m = Rc::new(Model::from_value(v)?);
    MODELS.with(|ms| ms.borrow_mut().insert(table, m.clone()));
    Ok(m)
}

/// A model known by its table (for relations).
pub fn model_named(table: &str) -> Option<Rc<Model>> {
    MODELS.with(|m| m.borrow().get(table).cloned())
}

/// Forgets what was read of a table's model (it is being defined again).
pub fn forget(table: &str) {
    MODELS.with(|m| m.borrow_mut().remove(table));
    super::host::forget_shape(table);
}
