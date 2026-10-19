//! Facile: the full-stack web framework for the Rythme language.
//!
//! Applications are written in Rythme (`app.ry`); this crate is the server
//! that runs them: the engine ([`server`]), the functions Rythme code calls
//! ([`host`]), the middlewares of `mw` ([`natives`]), the ORM ([`orm`]).

pub mod assets;
pub mod host;
pub mod http;
pub mod natives;
pub mod orm;
pub mod router;
pub mod runtime;
pub mod server;
pub mod statics;

pub use facile_core;
pub use rythme;

/// The stack of each worker: Rythme functions may recurse deeply.
pub const STACK_SIZE: usize = 256 << 20;
