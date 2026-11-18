//! Embeds build metadata (build number, git commit, date) into the binary.
//!
//! CI sets `BLURLIST_BUILD_NUMBER` to the workflow run number; local builds
//! report `dev`.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=BLURLIST_BUILD_NUMBER");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    for p in ["../../.git/HEAD", "../../.git/refs/heads", "../../.git/refs/tags"] {
        if std::path::Path::new(p).exists() {
            println!("cargo:rerun-if-changed={p}");
        }
    }

    let build =
        std::env::var("BLURLIST_BUILD_NUMBER").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "dev".into());

    let sha = std::env::var("GITHUB_SHA")
        .ok()
        .map(|s| s.chars().take(7).collect::<String>())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            Command::new("git")
                .args(["rev-parse", "--short=7", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        })
        .unwrap_or_else(|| "unknown".into());

    let secs = std::env::var("SOURCE_DATE_EPOCH").ok().and_then(|s| s.parse::<i64>().ok()).unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
    });

    println!("cargo:rustc-env=BLURLIST_BUILD_NUMBER={build}");
    println!("cargo:rustc-env=BLURLIST_GIT_SHA={sha}");
    println!("cargo:rustc-env=BLURLIST_BUILD_DATE={}", civil_date(secs));
}

/// Days-since-epoch to (Y-M-D) without pulling in a date crate.
fn civil_date(secs: i64) -> String {
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}
