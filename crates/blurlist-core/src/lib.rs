//! # blurlist-core
//!
//! The engine behind [Blurlist](https://github.com/lobojoel2003/Blurlist): it
//! finds the faces of opted-out people across a folder of event photos and
//! exports copies with those faces irreversibly blurred and all metadata
//! stripped. Everything runs locally with ONNX Runtime; nothing is uploaded.
//!
//! The typical flow is:
//!
//! 1. [`roster::add_reference_photo`] / [`roster::import_roster_folder`] — build
//!    the opt-out roster from one reference photo per person.
//! 2. [`scan::run_scan`] — detect and match every face in an event folder.
//! 3. [`store::Store::review_items`] / [`store::Store::set_decision`] — a person
//!    confirms matches and resolves the "Check these" pile.
//! 4. [`export::export_scan`] — write the blurred, metadata-free copies.

pub mod blur;
pub mod detect;
pub mod embed;
pub mod engine;
pub mod error;
pub mod export;
pub mod geometry;
pub mod imageio;
pub mod matching;
pub mod models;
pub mod roster;
pub mod scan;
pub mod settings;
pub mod store;

pub use engine::Engine;
pub use error::{Error, Result};
pub use settings::Settings;
pub use store::Store;

/// Semantic version (kept in lockstep with the app version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// CI build number (`dev` for local builds).
pub const BUILD_NUMBER: &str = env!("BLURLIST_BUILD_NUMBER");
/// Short git commit the binary was built from.
pub const GIT_SHA: &str = env!("BLURLIST_GIT_SHA");
/// UTC build date (YYYY-MM-DD).
pub const BUILD_DATE: &str = env!("BLURLIST_BUILD_DATE");
/// `0.1.0 (build 42, 1a2b3c4)`.
pub const LONG_VERSION: &str =
    concat!(env!("CARGO_PKG_VERSION"), " (build ", env!("BLURLIST_BUILD_NUMBER"), ", ", env!("BLURLIST_GIT_SHA"), ")");
