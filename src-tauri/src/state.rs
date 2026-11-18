//! Shared application state: the database, the lazily loaded engine and the
//! single long-running task slot.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use blurlist_core::models::{discover_models, ModelInfo, ModelKind, DEFAULT_DETECTOR, DEFAULT_EMBEDDER};
use blurlist_core::{Engine, Settings, Store};
use parking_lot::Mutex;

pub struct AppState {
    pub store: Arc<Store>,
    pub data_dir: PathBuf,
    /// Where imported models are copied.
    pub user_models_dir: PathBuf,
    /// Searched in order for model files.
    pub model_dirs: Vec<PathBuf>,
    engine: Mutex<Option<Arc<Engine>>>,
    /// Message explaining why the configured models could not be used.
    pub engine_notice: Mutex<Option<String>>,
    pub cancel: Arc<AtomicBool>,
    busy: Mutex<Option<&'static str>>,
    preview_cache: Mutex<VecDeque<(i64, Arc<Vec<u8>>)>>,
}

/// Held while a long task runs; frees the slot when dropped.
pub struct BusyGuard<'a>(&'a AppState);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        *self.0.busy.lock() = None;
    }
}

const PREVIEW_CACHE: usize = 24;

impl AppState {
    pub fn new(store: Store, data_dir: PathBuf, model_dirs: Vec<PathBuf>, user_models_dir: PathBuf) -> Self {
        Self {
            store: Arc::new(store),
            data_dir,
            user_models_dir,
            model_dirs,
            engine: Mutex::new(None),
            engine_notice: Mutex::new(None),
            cancel: Arc::new(AtomicBool::new(false)),
            busy: Mutex::new(None),
            preview_cache: Mutex::new(VecDeque::new()),
        }
    }

    pub fn models(&self) -> Vec<ModelInfo> {
        discover_models(&self.model_dirs)
    }

    /// The loaded engine, loading it on first use.
    pub fn engine(&self) -> Result<Arc<Engine>, String> {
        let mut guard = self.engine.lock();
        if let Some(e) = guard.as_ref() {
            return Ok(e.clone());
        }
        let settings = self.store.settings().map_err(|e| e.to_string())?;
        let engine = Arc::new(self.load_engine(&settings)?);
        *guard = Some(engine.clone());
        Ok(engine)
    }

    /// Drop the loaded engine so the next use picks up new settings.
    pub fn reset_engine(&self) {
        *self.engine.lock() = None;
        *self.engine_notice.lock() = None;
    }

    fn load_engine(&self, settings: &Settings) -> Result<Engine, String> {
        let models = self.models();
        let find = |id: &str, kind: ModelKind| {
            models.iter().find(|m| m.id == id && m.kind == kind && m.path.is_some()).cloned()
        };
        let mut notices = Vec::new();
        let det = match find(&settings.detector_id, ModelKind::Detector) {
            Some(m) => m,
            None => {
                notices
                    .push(format!("The detector \"{}\" was not found, so the default is used.", settings.detector_id));
                find(DEFAULT_DETECTOR, ModelKind::Detector)
                    .ok_or("The bundled face detector is missing. Please reinstall Blurlist.")?
            }
        };
        let emb = match find(&settings.embedder_id, ModelKind::Embedder) {
            Some(m) => m,
            None => {
                notices.push(format!(
                    "The recogniser \"{}\" was not found, so the default is used.",
                    settings.embedder_id
                ));
                find(DEFAULT_EMBEDDER, ModelKind::Embedder)
                    .ok_or("The bundled face recogniser is missing. Please reinstall Blurlist.")?
            }
        };
        *self.engine_notice.lock() = if notices.is_empty() { None } else { Some(notices.join(" ")) };
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        Engine::load(&det, &emb, threads).map_err(|e| e.to_string())
    }

    /// Claim the long-task slot.
    pub fn begin(&self, what: &'static str) -> Result<BusyGuard<'_>, String> {
        let mut busy = self.busy.lock();
        if let Some(current) = *busy {
            return Err(format!("Please wait: Blurlist is still busy with {current}."));
        }
        *busy = Some(what);
        self.cancel.store(false, Ordering::SeqCst);
        Ok(BusyGuard(self))
    }

    pub fn busy_with(&self) -> Option<&'static str> {
        *self.busy.lock()
    }

    pub fn cached_preview(&self, photo_id: i64) -> Option<Arc<Vec<u8>>> {
        self.preview_cache.lock().iter().find(|(id, _)| *id == photo_id).map(|(_, b)| b.clone())
    }

    pub fn cache_preview(&self, photo_id: i64, bytes: Arc<Vec<u8>>) {
        let mut cache = self.preview_cache.lock();
        cache.retain(|(id, _)| *id != photo_id);
        cache.push_front((photo_id, bytes));
        cache.truncate(PREVIEW_CACHE);
    }

    pub fn clear_previews(&self) {
        self.preview_cache.lock().clear();
    }
}
