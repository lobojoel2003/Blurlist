//! The `media://` protocol serves face crops, reference photos and previews
//! straight from the local database or disk to the web view. It never serves
//! arbitrary files: only ids of rows that already exist.
//!
//! Paths: `ref/<id>`, `refface/<id>`, `face/<id>`, `thumb/<photo id>`, `photo/<photo id>`.

use std::path::PathBuf;
use std::sync::Arc;

use blurlist_core::imageio::{encode_jpeg, fit_within, load_image};
use blurlist_core::roster::reference_face_view;
use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::state::AppState;

const PREVIEW_SIDE: u32 = 1600;

pub fn handle<R: Runtime>(ctx: UriSchemeContext<'_, R>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().clone();
    let path = request.uri().path().to_string();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let response = match serve(&state, &path) {
            Ok(bytes) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "image/jpeg")
                .header(header::CACHE_CONTROL, "no-store")
                .header("Access-Control-Allow-Origin", "*")
                .body(bytes),
            Err(msg) => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(header::CONTENT_TYPE, "text/plain")
                .body(msg.into_bytes()),
        };
        responder.respond(response.unwrap_or_else(|_| Response::new(Vec::new())));
    });
}

fn serve(state: &AppState, raw_path: &str) -> Result<Vec<u8>, String> {
    let decoded = percent_encoding::percent_decode_str(raw_path).decode_utf8_lossy();
    let path = decoded.trim_start_matches('/');
    let (kind, id) = path.split_once('/').ok_or("bad media path")?;
    let id: i64 = id.split(['?', '/']).next().unwrap_or("").parse().map_err(|_| "bad media id".to_string())?;
    let store = &state.store;
    match kind {
        "ref" => store.reference_crop(id).map_err(|e| e.to_string()),
        "refface" => {
            let r = store.reference(id).map_err(|e| e.to_string())?;
            reference_face_view(&r.crop_jpeg, &r.crop_landmarks).map_err(|e| e.to_string())
        }
        "face" => store.face_crop(id).map_err(|e| e.to_string()),
        "thumb" => store.photo_thumb(id).map_err(|e| e.to_string()),
        "photo" => {
            if let Some(b) = state.cached_preview(id) {
                return Ok(b.as_ref().clone());
            }
            let photo = store.photo(id).map_err(|e| e.to_string())?;
            let scan = store.scan(photo.scan_id).map_err(|e| e.to_string())?;
            let file = photo.rel_path.split('/').fold(PathBuf::from(&scan.input_dir), |acc, part| acc.join(part));
            let img = load_image(&file).map_err(|e| e.to_string())?;
            let bytes = encode_jpeg(&fit_within(&img.rgb, PREVIEW_SIDE), 86).map_err(|e| e.to_string())?;
            let bytes = Arc::new(bytes);
            state.cache_preview(id, bytes.clone());
            Ok(bytes.as_ref().clone())
        }
        _ => Err("unknown media kind".into()),
    }
}
