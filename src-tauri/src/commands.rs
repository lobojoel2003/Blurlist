//! Commands invoked by the web view. Heavy work runs on blocking threads so
//! the UI stays responsive; progress is reported with events.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use base64::Engine as _;
use blurlist_core::export::{export_scan as core_export, ExportOptions, ExportReport};
use blurlist_core::geometry::BBox;
use blurlist_core::matching::{Decision, Pile};
use blurlist_core::models::{import_model as core_import_model, ModelFamily, ModelInfo, Thresholds};
use blurlist_core::roster::{add_reference_photo, import_roster_folder, inspect_reference_photo, ImportReport};
use blurlist_core::scan::{rematch_scan, run_scan, RematchReport};
use blurlist_core::store::{FaceRecord, Person, PhotoRecord, ReviewItem, ScanSummary};
use blurlist_core::{Settings, BUILD_DATE, BUILD_NUMBER, GIT_SHA, VERSION};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Run `f` on a blocking thread with access to the app state.
async fn blocking<T, F>(app: &AppHandle, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppState, &AppHandle) -> CmdResult<T> + Send + 'static,
{
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        f(&state, &app)
    })
    .await
    .map_err(err)?
}

// ------------------------------------------------------------------ app info

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    detector: ModelInfo,
    embedder: ModelInfo,
    default_thresholds: Thresholds,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    build_number: &'static str,
    git_sha: &'static str,
    build_date: &'static str,
    data_dir: PathBuf,
    user_models_dir: PathBuf,
    engine: Option<EngineInfo>,
    engine_error: Option<String>,
    engine_notice: Option<String>,
    busy: Option<&'static str>,
}

#[tauri::command]
pub async fn app_info(app: AppHandle) -> CmdResult<AppInfo> {
    blocking(&app, |s, _| {
        let (engine, engine_error) = match s.engine() {
            Ok(e) => (
                Some(EngineInfo {
                    detector: e.detector_info.clone(),
                    embedder: e.embedder_info.clone(),
                    default_thresholds: e.default_thresholds(),
                }),
                None,
            ),
            Err(e) => (None, Some(e)),
        };
        Ok(AppInfo {
            version: VERSION,
            build_number: BUILD_NUMBER,
            git_sha: GIT_SHA,
            build_date: BUILD_DATE,
            data_dir: s.data_dir.clone(),
            user_models_dir: s.user_models_dir.clone(),
            engine,
            engine_error,
            engine_notice: s.engine_notice.lock().clone(),
            busy: s.busy_with(),
        })
    })
    .await
}

#[tauri::command]
pub async fn list_models(app: AppHandle) -> CmdResult<Vec<ModelInfo>> {
    blocking(&app, |s, _| Ok(s.models())).await
}

#[tauri::command]
pub async fn import_model(
    app: AppHandle,
    path: PathBuf,
    name: String,
    family: ModelFamily,
    license: String,
) -> CmdResult<Vec<ModelInfo>> {
    blocking(&app, move |s, _| {
        core_import_model(&path, &s.user_models_dir, &name, family, &license).map_err(err)?;
        Ok(s.models())
    })
    .await
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> CmdResult<Settings> {
    blocking(&app, |s, _| s.store.settings().map_err(err)).await
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> CmdResult<Settings> {
    blocking(&app, move |s, _| {
        let old = s.store.settings().map_err(err)?;
        s.store.save_settings(&settings).map_err(err)?;
        if old.detector_id != settings.detector_id || old.embedder_id != settings.embedder_id {
            s.reset_engine();
        }
        s.store.settings().map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn reveal_path(app: AppHandle, path: PathBuf) -> CmdResult<()> {
    if path.is_dir() {
        app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(err)
    } else {
        app.opener().reveal_item_in_dir(&path).map_err(err)
    }
}

/// Open a web page (model licences and sources) in the user's browser.
#[tauri::command]
pub async fn open_external(app: AppHandle, url: String) -> CmdResult<()> {
    if !url.starts_with("https://") {
        return Err("only https links can be opened".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(err)
}

#[tauri::command]
pub fn cancel_task(app: AppHandle) {
    app.state::<AppState>().cancel.store(true, Ordering::SeqCst);
}

// -------------------------------------------------------------------- roster

#[tauri::command]
pub async fn list_people(app: AppHandle) -> CmdResult<Vec<Person>> {
    blocking(&app, |s, _| s.store.list_people().map_err(err)).await
}

#[tauri::command]
pub async fn create_person(app: AppHandle, name: String, group: String, notes: String) -> CmdResult<i64> {
    blocking(&app, move |s, _| s.store.add_person(&name, &group, &notes).map_err(err)).await
}

#[tauri::command]
pub async fn update_person(app: AppHandle, id: i64, name: String, group: String, notes: String) -> CmdResult<()> {
    blocking(&app, move |s, _| s.store.update_person(id, &name, &group, &notes).map_err(err)).await
}

#[tauri::command]
pub async fn delete_person(app: AppHandle, id: i64) -> CmdResult<()> {
    blocking(&app, move |s, _| s.store.delete_person(id).map_err(err)).await
}

#[tauri::command]
pub async fn delete_reference(app: AppHandle, id: i64) -> CmdResult<()> {
    blocking(&app, move |s, _| s.store.delete_reference(id).map_err(err)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateDto {
    index: usize,
    bbox: BBox,
    score: f32,
    face_px: u32,
    warnings: Vec<String>,
    preview: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionDto {
    width: u32,
    height: u32,
    suggested: Option<usize>,
    faces: Vec<CandidateDto>,
}

#[tauri::command]
pub async fn inspect_photo(app: AppHandle, path: PathBuf) -> CmdResult<InspectionDto> {
    blocking(&app, move |s, _| {
        let engine = s.engine()?;
        let settings = s.store.settings().map_err(err)?;
        let insp = inspect_reference_photo(&engine, &path, &settings).map_err(err)?;
        Ok(InspectionDto {
            width: insp.width,
            height: insp.height,
            suggested: insp.suggested,
            faces: insp
                .faces
                .into_iter()
                .map(|f| CandidateDto {
                    index: f.index,
                    bbox: f.bbox,
                    score: f.score,
                    face_px: f.face_px,
                    warnings: f.warnings,
                    preview: format!(
                        "data:image/jpeg;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(&f.crop_jpeg)
                    ),
                })
                .collect(),
        })
    })
    .await
}

#[tauri::command]
pub async fn add_reference(app: AppHandle, person_id: i64, path: PathBuf, face_index: Option<usize>) -> CmdResult<i64> {
    blocking(&app, move |s, _| {
        let engine = s.engine()?;
        let settings = s.store.settings().map_err(err)?;
        add_reference_photo(&engine, &s.store, person_id, &path, face_index, &settings).map_err(err)
    })
    .await
}

#[derive(Clone, Serialize)]
struct CountProgress {
    done: usize,
    total: usize,
}

#[tauri::command]
pub async fn import_roster(app: AppHandle, folder: PathBuf) -> CmdResult<ImportReport> {
    blocking(&app, move |s, app| {
        let _busy = s.begin("importing the roster")?;
        let engine = s.engine()?;
        let settings = s.store.settings().map_err(err)?;
        import_roster_folder(&engine, &s.store, &folder, &settings, &|done, total| {
            let _ = app.emit("import-progress", CountProgress { done, total });
        })
        .map_err(err)
    })
    .await
}

// --------------------------------------------------------------------- scans

#[tauri::command]
pub async fn start_scan(app: AppHandle, name: String, folder: PathBuf) -> CmdResult<i64> {
    blocking(&app, move |s, app| {
        let _busy = s.begin("a scan")?;
        let engine = s.engine()?;
        let settings = s.store.settings().map_err(err)?;
        let name = if name.trim().is_empty() {
            folder.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Event".into())
        } else {
            name.trim().to_string()
        };
        run_scan(
            &engine,
            &s.store,
            &name,
            &folder,
            &settings,
            &|p| {
                let _ = app.emit("scan-progress", p);
            },
            &s.cancel,
        )
        .map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn list_scans(app: AppHandle) -> CmdResult<Vec<ScanSummary>> {
    blocking(&app, |s, _| s.store.list_scans().map_err(err)).await
}

#[tauri::command]
pub async fn get_scan(app: AppHandle, id: i64) -> CmdResult<ScanSummary> {
    blocking(&app, move |s, _| s.store.scan(id).map_err(err)).await
}

#[tauri::command]
pub async fn delete_scan(app: AppHandle, id: i64) -> CmdResult<()> {
    blocking(&app, move |s, _| {
        s.clear_previews();
        s.store.delete_scan(id).map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn rematch(app: AppHandle, scan_id: i64) -> CmdResult<RematchReport> {
    blocking(&app, move |s, _| {
        let _busy = s.begin("re-matching")?;
        let engine = s.engine()?;
        let settings = s.store.settings().map_err(err)?;
        rematch_scan(&engine, &s.store, scan_id, &settings).map_err(err)
    })
    .await
}

// -------------------------------------------------------------------- review

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum PileArg {
    Matches,
    Check,
}

impl From<PileArg> for Pile {
    fn from(p: PileArg) -> Self {
        match p {
            PileArg::Matches => Pile::Matches,
            PileArg::Check => Pile::Check,
        }
    }
}

#[tauri::command]
pub async fn review_items(app: AppHandle, scan_id: i64, pile: PileArg) -> CmdResult<Vec<ReviewItem>> {
    blocking(&app, move |s, _| s.store.review_items(scan_id, pile.into()).map_err(err)).await
}

#[tauri::command]
pub async fn set_decision(app: AppHandle, face_id: i64, decision: Decision) -> CmdResult<FaceRecord> {
    blocking(&app, move |s, _| s.store.set_decision(face_id, decision).map_err(err)).await
}

#[tauri::command]
pub async fn restore_decision(
    app: AppHandle,
    face_id: i64,
    decision: Decision,
    reviewed: bool,
) -> CmdResult<FaceRecord> {
    blocking(&app, move |s, _| s.store.restore_decision(face_id, decision, reviewed).map_err(err)).await
}

#[tauri::command]
pub async fn bulk_decision(
    app: AppHandle,
    scan_id: i64,
    pile: PileArg,
    only_pending: bool,
    decision: Decision,
) -> CmdResult<usize> {
    blocking(&app, move |s, _| s.store.bulk_decision(scan_id, pile.into(), only_pending, decision).map_err(err)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceWithPerson {
    #[serde(flatten)]
    face: FaceRecord,
    person_name: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoDetail {
    photo: PhotoRecord,
    faces: Vec<FaceWithPerson>,
}

#[tauri::command]
pub async fn photo_detail(app: AppHandle, photo_id: i64) -> CmdResult<PhotoDetail> {
    blocking(&app, move |s, _| {
        let photo = s.store.photo(photo_id).map_err(err)?;
        let people = s.store.list_people().map_err(err)?;
        let faces = s
            .store
            .photo_faces(photo_id)
            .map_err(err)?
            .into_iter()
            .map(|f| FaceWithPerson {
                person_name: f.person_id.and_then(|pid| people.iter().find(|p| p.id == pid).map(|p| p.name.clone())),
                face: f,
            })
            .collect();
        Ok(PhotoDetail { photo, faces })
    })
    .await
}

#[tauri::command]
pub async fn add_manual_region(app: AppHandle, photo_id: i64, bbox: BBox) -> CmdResult<i64> {
    blocking(&app, move |s, _| {
        let photo = s.store.photo(photo_id).map_err(err)?;
        let b = bbox.clamp(photo.width as f32, photo.height as f32);
        if b.w < 4.0 || b.h < 4.0 {
            return Err("Draw a larger region.".into());
        }
        s.store.add_manual_face(photo_id, b, None).map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn delete_manual_region(app: AppHandle, face_id: i64) -> CmdResult<()> {
    blocking(&app, move |s, _| s.store.delete_manual_face(face_id).map_err(err)).await
}

// -------------------------------------------------------------------- export

#[tauri::command]
pub async fn export_scan(app: AppHandle, scan_id: i64, destination: PathBuf) -> CmdResult<ExportReport> {
    blocking(&app, move |s, app| {
        let _busy = s.begin("an export")?;
        let settings = s.store.settings().map_err(err)?;
        let opts = ExportOptions { destination, blur_style: settings.blur_style, jpeg_quality: settings.jpeg_quality };
        core_export(
            &s.store,
            scan_id,
            &opts,
            &|p| {
                let _ = app.emit("export-progress", p);
            },
            &s.cancel,
        )
        .map_err(err)
    })
    .await
}
