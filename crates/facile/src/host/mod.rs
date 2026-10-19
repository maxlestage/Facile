//! The functions Facile adds to Rythme.

pub mod app;
pub mod chain;
pub mod mw;
pub mod page;
pub mod pure;
pub mod request;
pub mod respond;

use rythme::interp::Interp;

/// Defines every function of Facile in an interpreter (and so for its checker).
pub fn install(it: &mut Interp) {
    pure::install(it);
    facile_core::view::define(it);
    facile_core::util::define(it, app::APP);
    facile_core::util::define(it, respond::RESPOND);
    facile_core::util::define(it, page::PAGE);
    facile_core::util::namespace(it, "mw", mw::MW);
    crate::orm::host::install(it);
}
