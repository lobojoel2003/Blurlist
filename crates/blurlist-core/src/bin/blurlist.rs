//! `blurlist` — command-line front end to the Blurlist engine.
//!
//! Shares its database format with the desktop app, so a roster built in one
//! can be used from the other.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use anyhow::{bail, Context, Result};
use blurlist_core::blur::BlurStyle;
use blurlist_core::detect::Thoroughness;
use blurlist_core::export::{export_scan, ExportOptions};
use blurlist_core::matching::{Decision, Pile};
use blurlist_core::models::{discover_models, ModelInfo, ModelKind};
use blurlist_core::roster::{add_reference_photo, import_roster_folder, inspect_reference_photo, ImportOutcome};
use blurlist_core::scan::{rematch_scan, run_scan};
use blurlist_core::{Engine, Settings, Store};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "blurlist", version = version_string(), about = "Find and blur opted-out faces in event photos, offline.")]
struct Cli {
    /// Database file holding the roster and scans.
    #[arg(long, global = true, env = "BLURLIST_DB", default_value = "blurlist.sqlite3")]
    db: PathBuf,
    /// Folder(s) containing the ONNX models. Defaults to ./models and the folder next to the executable.
    #[arg(long, global = true, env = "BLURLIST_MODELS")]
    models: Vec<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage the opt-out roster.
    Roster {
        #[command(subcommand)]
        action: RosterCmd,
    },
    /// Scan an event folder for roster faces.
    Scan {
        folder: PathBuf,
        /// Event name (defaults to the folder name).
        #[arg(long)]
        name: Option<String>,
        #[arg(long, value_enum)]
        thoroughness: Option<ThoroughnessArg>,
    },
    /// List scans.
    Scans,
    /// Show the faces in a review pile.
    Review {
        scan: i64,
        #[arg(long, value_enum, default_value = "check")]
        pile: PileArg,
    },
    /// Decide what happens to a face.
    Decide {
        face: i64,
        #[arg(value_enum)]
        decision: DecisionArg,
    },
    /// Decide every face in a pile at once.
    DecideAll {
        scan: i64,
        #[arg(value_enum)]
        pile: PileArg,
        #[arg(value_enum)]
        decision: DecisionArg,
        /// Only faces still waiting for a decision.
        #[arg(long)]
        pending_only: bool,
    },
    /// Re-run matching for a scan after the roster or thresholds changed.
    Rematch { scan: i64 },
    /// Export blurred, metadata-free copies of a scanned event.
    Export {
        scan: i64,
        /// Folder in which the export folder is created.
        destination: PathBuf,
        #[arg(long, value_enum, default_value = "blur")]
        style: StyleArg,
        #[arg(long, default_value_t = 92)]
        quality: u8,
    },
    /// List known models and where they were found.
    Models,
}

#[derive(Subcommand)]
enum RosterCmd {
    /// Add a person with one or more reference photos.
    Add {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        group: String,
        #[arg(required = true)]
        photos: Vec<PathBuf>,
    },
    /// Import a folder of photos named after each person (sub-folders become groups).
    Import { folder: PathBuf },
    /// List everyone on the roster.
    List,
    /// Remove a person and their reference photos.
    Remove { id: i64 },
}

#[derive(Clone, Copy, ValueEnum)]
enum PileArg {
    Matches,
    Check,
}

#[derive(Clone, Copy, ValueEnum)]
enum DecisionArg {
    Blur,
    Keep,
}

#[derive(Clone, Copy, ValueEnum)]
enum StyleArg {
    Blur,
    Pixelate,
    Solid,
}

#[derive(Clone, Copy, ValueEnum)]
enum ThoroughnessArg {
    Fast,
    Thorough,
    Maximum,
}

fn version_string() -> &'static str {
    blurlist_core::LONG_VERSION
}

fn model_dirs(cli: &Cli) -> Vec<PathBuf> {
    let mut dirs = cli.models.clone();
    dirs.push(PathBuf::from("models"));
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        dirs.push(exe_dir.join("models"));
        dirs.push(exe_dir.join("../share/blurlist/models"));
    }
    dirs
}

fn pick(models: &[ModelInfo], id: &str, kind: ModelKind) -> Result<ModelInfo> {
    let m = models.iter().find(|m| m.id == id && m.kind == kind).with_context(|| format!("unknown model {id}"))?;
    if m.path.is_none() {
        bail!("model file {} not found; pass --models <dir> or run scripts/fetch-models.mjs", m.file);
    }
    Ok(m.clone())
}

fn engine(cli: &Cli, settings: &Settings) -> Result<Engine> {
    let models = discover_models(&model_dirs(cli));
    let det = pick(&models, &settings.detector_id, ModelKind::Detector)?;
    let emb = pick(&models, &settings.embedder_id, ModelKind::Embedder)?;
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    Ok(Engine::load(&det, &emb, threads)?)
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_env("BLURLIST_LOG"))
        .with_writer(std::io::stderr)
        .init();
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let store = Store::open(&cli.db).with_context(|| format!("opening {}", cli.db.display()))?;
    store.recover_interrupted_scans()?;
    let mut settings = store.settings()?;
    match &cli.command {
        Command::Roster { action } => match action {
            RosterCmd::Add { name, group, photos } => {
                let engine = engine(&cli, &settings)?;
                let pid = store.add_person(name, group, "")?;
                for photo in photos {
                    let insp = inspect_reference_photo(&engine, photo, &settings)?;
                    match add_reference_photo(&engine, &store, pid, photo, insp.suggested, &settings) {
                        Ok(_) => println!("added {} for {name}", photo.display()),
                        Err(e) => {
                            if store.list_people()?.iter().any(|p| p.id == pid && p.references.is_empty()) {
                                store.delete_person(pid)?;
                            }
                            bail!("{}: {e}", photo.display());
                        }
                    }
                }
                println!("{name} is on the roster (id {pid})");
            }
            RosterCmd::Import { folder } => {
                let engine = engine(&cli, &settings)?;
                let report = import_roster_folder(&engine, &store, folder, &settings, &|i, n| {
                    if n > 0 {
                        eprint!("\r{i}/{n}");
                    }
                })?;
                eprintln!();
                for e in &report.entries {
                    match &e.outcome {
                        ImportOutcome::Added { .. } => println!("  ok     {} → {}", e.file, e.name),
                        ImportOutcome::NeedsAttention { reason } => println!("  check  {}: {reason}", e.file),
                        ImportOutcome::Failed { error } => println!("  failed {}: {error}", e.file),
                    }
                }
                println!("{} people added, {} reference photos stored", report.people_created, report.photos_added);
            }
            RosterCmd::List => {
                let people = store.list_people()?;
                if people.is_empty() {
                    println!("The roster is empty.");
                }
                for p in people {
                    let group = if p.group.is_empty() { String::new() } else { format!(" [{}]", p.group) };
                    println!(
                        "{:>5}  {}{}  ({} photo{})",
                        p.id,
                        p.name,
                        group,
                        p.references.len(),
                        if p.references.len() == 1 { "" } else { "s" }
                    );
                }
            }
            RosterCmd::Remove { id } => {
                store.delete_person(*id)?;
                println!("removed {id}");
            }
        },
        Command::Scan { folder, name, thoroughness } => {
            if let Some(t) = thoroughness {
                settings.thoroughness = match t {
                    ThoroughnessArg::Fast => Thoroughness::Fast,
                    ThoroughnessArg::Thorough => Thoroughness::Thorough,
                    ThoroughnessArg::Maximum => Thoroughness::Maximum,
                };
            }
            if store.person_count()? == 0 {
                eprintln!("warning: the roster is empty, so nothing will be matched");
            }
            let engine = engine(&cli, &settings)?;
            let name = name.clone().unwrap_or_else(|| {
                folder.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Event".into())
            });
            // Ctrl-C ends the process; the interrupted scan is marked as failed
            // the next time the database is opened.
            let cancel = AtomicBool::new(false);
            let id = run_scan(
                &engine,
                &store,
                &name,
                folder,
                &settings,
                &|p| {
                    if p.total > 0 {
                        eprint!(
                            "\r{}/{} photos · {} faces · {} matches · {} to check   ",
                            p.done, p.total, p.faces, p.matches, p.check
                        );
                    }
                },
                &cancel,
            )?;
            eprintln!();
            print_scan(&store, id)?;
        }
        Command::Scans => {
            for s in store.list_scans()? {
                println!(
                    "{:>4}  {:<30} {:?}  {} photos, {} matches, {} to check",
                    s.id, s.name, s.status, s.counts.photos, s.counts.matches, s.counts.check_pending
                );
            }
        }
        Command::Review { scan, pile } => {
            let pile = match pile {
                PileArg::Matches => Pile::Matches,
                PileArg::Check => Pile::Check,
            };
            for it in store.review_items(*scan, pile)? {
                let who = it.person.as_ref().map(|p| p.name.as_str()).unwrap_or("—");
                println!(
                    "face {:>6}  {:<40} {:<20} sim {:>5}  {:?}{}",
                    it.face.id,
                    it.photo.rel_path,
                    who,
                    it.face.similarity.map(|s| format!("{s:.2}")).unwrap_or_default(),
                    it.face.decision,
                    if it.face.reviewed { " ✓" } else { "" }
                );
            }
        }
        Command::Decide { face, decision } => {
            let d = match decision {
                DecisionArg::Blur => Decision::Blur,
                DecisionArg::Keep => Decision::Keep,
            };
            store.set_decision(*face, d)?;
            println!("face {face}: {d:?}");
        }
        Command::DecideAll { scan, pile, decision, pending_only } => {
            let p = match pile {
                PileArg::Matches => Pile::Matches,
                PileArg::Check => Pile::Check,
            };
            let d = match decision {
                DecisionArg::Blur => Decision::Blur,
                DecisionArg::Keep => Decision::Keep,
            };
            let n = store.bulk_decision(*scan, p, *pending_only, d)?;
            println!("{n} faces set to {d:?}");
        }
        Command::Rematch { scan } => {
            let engine = engine(&cli, &settings)?;
            let r = rematch_scan(&engine, &store, *scan, &settings)?;
            println!("{} faces checked, {} need review again", r.faces_checked, r.changed);
            print_scan(&store, *scan)?;
        }
        Command::Export { scan, destination, style, quality } => {
            std::fs::create_dir_all(destination)?;
            let opts = ExportOptions {
                destination: destination.clone(),
                blur_style: match style {
                    StyleArg::Blur => BlurStyle::Blur,
                    StyleArg::Pixelate => BlurStyle::Pixelate,
                    StyleArg::Solid => BlurStyle::Solid,
                },
                jpeg_quality: *quality,
            };
            let cancel = AtomicBool::new(false);
            let r = export_scan(&store, *scan, &opts, &|p| eprint!("\r{}/{}   ", p.done, p.total), &cancel)?;
            eprintln!();
            println!("Exported {} photos to {}", r.exported, r.output_dir.display());
            println!("  {} faces blurred in {} photos", r.faces_blurred, r.photos_blurred);
            for h in &r.held_back {
                println!("  held back  {}: {}", h.rel_path, h.reason);
            }
            for f in &r.failed {
                println!("  failed     {}: {}", f.rel_path, f.reason);
            }
            for n in &r.not_supported {
                println!("  not checked (unsupported type)  {n}");
            }
            if !r.held_back.is_empty() {
                println!("Resolve the held-back photos with `blurlist review {scan} --pile check`, then export again.");
            }
        }
        Command::Models => {
            for m in discover_models(&model_dirs(&cli)) {
                println!(
                    "{:<20} {:<9} {:<32} {}{}",
                    m.id,
                    format!("{:?}", m.kind).to_lowercase(),
                    m.license,
                    m.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(not found)".into()),
                    if m.id == settings.detector_id || m.id == settings.embedder_id { "  [active]" } else { "" }
                );
            }
        }
    }
    Ok(())
}

fn print_scan(store: &Store, id: i64) -> Result<()> {
    let s = store.scan(id)?;
    let c = &s.counts;
    println!("Scan {id} \"{}\": {:?}", s.name, s.status);
    println!("  {} photos checked, {} not supported, {} unreadable", c.photos_ok, c.unsupported, c.errors);
    println!("  {} faces · {} roster matches · {} to check", c.faces, c.matches, c.check_pending);
    if c.photos_held > 0 {
        println!("  {} photos will be held back until the \"check\" pile is resolved", c.photos_held);
    }
    Ok(())
}
