//! User settings, persisted in the database.

use serde::{Deserialize, Serialize};

use crate::blur::BlurStyle;
use crate::detect::{DetectOptions, Thoroughness};
use crate::matching::{MatchConfig, SmallFacePolicy};
use crate::models::{Thresholds, DEFAULT_DETECTOR, DEFAULT_EMBEDDER};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub detector_id: String,
    pub embedder_id: String,
    /// Similarity thresholds; `None` uses the recogniser's defaults.
    pub thresholds: Option<Thresholds>,
    /// Faces smaller than this (shorter side, pixels) are "too small to identify".
    pub min_face_px: u32,
    pub small_face_policy: SmallFacePolicy,
    pub thoroughness: Thoroughness,
    pub detection_threshold: f32,
    pub blur_style: BlurStyle,
    pub jpeg_quality: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            detector_id: DEFAULT_DETECTOR.into(),
            embedder_id: DEFAULT_EMBEDDER.into(),
            thresholds: None,
            min_face_px: 36,
            small_face_policy: SmallFacePolicy::Review,
            thoroughness: Thoroughness::Thorough,
            detection_threshold: 0.5,
            blur_style: BlurStyle::Blur,
            jpeg_quality: 92,
        }
    }
}

impl Settings {
    pub fn detect_options(&self) -> DetectOptions {
        DetectOptions {
            threshold: self.detection_threshold.clamp(0.05, 0.99),
            thoroughness: self.thoroughness,
            ..DetectOptions::default()
        }
    }

    pub fn match_config(&self, model_defaults: Thresholds) -> MatchConfig {
        let mut t = self.thresholds.unwrap_or(model_defaults);
        t.review = t.review.clamp(-1.0, 1.0);
        t.matched = t.matched.clamp(t.review, 1.0);
        MatchConfig { thresholds: t, min_face_px: self.min_face_px as f32, small_policy: self.small_face_policy }
    }

    /// Reject values that would make the tool unsafe or unusable.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(t) = self.thresholds {
            if !(t.review.is_finite() && t.matched.is_finite()) || t.review > t.matched {
                return Err("the review threshold must not be above the match threshold".into());
            }
            if t.review < 0.0 || t.matched > 1.0 {
                return Err("thresholds must be between 0 and 1".into());
            }
        }
        if !(0.05..=0.99).contains(&self.detection_threshold) {
            return Err("detection threshold must be between 0.05 and 0.99".into());
        }
        if !(50..=100).contains(&self.jpeg_quality) {
            return Err("JPEG quality must be between 50 and 100".into());
        }
        if self.min_face_px > 400 {
            return Err("minimum face size is unreasonably large".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_roundtrip() {
        let s = Settings::default();
        assert!(s.validate().is_ok());
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
        // Missing keys fall back to defaults so old databases keep working.
        let partial: Settings = serde_json::from_str(r#"{"jpegQuality": 80}"#).unwrap();
        assert_eq!(partial.jpeg_quality, 80);
        assert_eq!(partial.detector_id, DEFAULT_DETECTOR);
    }

    #[test]
    fn rejects_inverted_thresholds() {
        let s = Settings { thresholds: Some(Thresholds { review: 0.6, matched: 0.4 }), ..Default::default() };
        assert!(s.validate().is_err());
    }
}
