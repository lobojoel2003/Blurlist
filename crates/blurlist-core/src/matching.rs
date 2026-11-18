//! Comparing detected faces with the roster and deciding what happens to them.

use serde::{Deserialize, Serialize};

use crate::embed::cosine;
use crate::models::Thresholds;

/// What the engine concluded about a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FaceCategory {
    /// Confidently the same person as someone on the roster.
    Match,
    /// Looks somewhat like someone on the roster; a person has to decide.
    Uncertain,
    /// Too small to identify reliably.
    Small,
    /// Not similar to anyone on the roster.
    None,
    /// Drawn by hand by the organiser.
    Manual,
}

/// What will happen to a face on export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// Waiting for a person. Photos with pending faces are never exported.
    Pending,
    Blur,
    Keep,
}

/// Which review list a face appears in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pile {
    /// Confident roster matches, blurred unless the organiser rejects them.
    Matches,
    /// "Check these": uncertain matches and unidentifiable faces.
    Check,
    /// Not listed for review.
    None,
}

/// What to do with faces too small to identify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SmallFacePolicy {
    /// Put them in the "Check these" pile.
    #[default]
    Review,
    /// Blur them all without asking.
    Blur,
    /// Leave them as they are.
    Ignore,
}

#[derive(Debug, Clone, Copy)]
pub struct MatchConfig {
    pub thresholds: Thresholds,
    /// Faces whose shorter box side is below this many pixels count as small.
    pub min_face_px: f32,
    pub small_policy: SmallFacePolicy,
}

/// Smallest face we even try to embed.
pub const MIN_EMBED_PX: f32 = 12.0;

/// Roster embeddings for one recogniser, grouped by person.
#[derive(Debug, Default, Clone)]
pub struct RosterIndex {
    people: Vec<(i64, Vec<Vec<f32>>)>,
}

impl RosterIndex {
    pub fn from_pairs(pairs: impl IntoIterator<Item = (i64, Vec<f32>)>) -> Self {
        let mut people: Vec<(i64, Vec<Vec<f32>>)> = Vec::new();
        for (pid, v) in pairs {
            match people.iter_mut().find(|(p, _)| *p == pid) {
                Some((_, list)) => list.push(v),
                None => people.push((pid, vec![v])),
            }
        }
        Self { people }
    }

    pub fn is_empty(&self) -> bool {
        self.people.is_empty()
    }

    pub fn len(&self) -> usize {
        self.people.len()
    }

    /// Best-matching person and similarity (max over their reference photos).
    pub fn best(&self, embedding: &[f32]) -> Option<(i64, f32)> {
        self.people
            .iter()
            .map(|(pid, refs)| {
                let s = refs.iter().map(|r| cosine(r, embedding)).fold(f32::NEG_INFINITY, f32::max);
                (*pid, s)
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Classification {
    pub category: FaceCategory,
    pub pile: Pile,
    pub person_id: Option<i64>,
    pub similarity: Option<f32>,
    pub decision: Decision,
}

/// Classify one face. Uncertain cases are never resolved automatically: they
/// become `Pending`, which blocks the photo from export until someone looks.
pub fn classify(face_px: f32, embedding: Option<&[f32]>, roster: &RosterIndex, cfg: &MatchConfig) -> Classification {
    let best = embedding.and_then(|e| roster.best(e));
    let big_enough = face_px >= cfg.min_face_px;
    if let Some((pid, sim)) = best {
        if big_enough && sim >= cfg.thresholds.matched {
            return Classification {
                category: FaceCategory::Match,
                pile: Pile::Matches,
                person_id: Some(pid),
                similarity: Some(sim),
                decision: Decision::Blur,
            };
        }
        if sim >= cfg.thresholds.review {
            return Classification {
                category: FaceCategory::Uncertain,
                pile: Pile::Check,
                person_id: Some(pid),
                similarity: Some(sim),
                decision: Decision::Pending,
            };
        }
    }
    let similarity = best.map(|b| b.1);
    if !big_enough {
        let (pile, decision) = match cfg.small_policy {
            SmallFacePolicy::Review => (Pile::Check, Decision::Pending),
            SmallFacePolicy::Blur => (Pile::None, Decision::Blur),
            SmallFacePolicy::Ignore => (Pile::None, Decision::Keep),
        };
        return Classification { category: FaceCategory::Small, pile, person_id: None, similarity, decision };
    }
    Classification {
        category: FaceCategory::None,
        pile: Pile::None,
        person_id: None,
        similarity,
        decision: Decision::Keep,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::l2_normalize;

    fn cfg(policy: SmallFacePolicy) -> MatchConfig {
        MatchConfig { thresholds: Thresholds { review: 0.3, matched: 0.5 }, min_face_px: 40.0, small_policy: policy }
    }

    fn roster() -> RosterIndex {
        RosterIndex::from_pairs([
            (1, l2_normalize(vec![1.0, 0.0, 0.0])),
            (2, l2_normalize(vec![0.0, 1.0, 0.0])),
            (2, l2_normalize(vec![0.0, 0.9, 0.1])),
        ])
    }

    #[test]
    fn confident_match_is_blurred_and_listed() {
        let e = l2_normalize(vec![0.05, 1.0, 0.0]);
        let c = classify(80.0, Some(&e), &roster(), &cfg(SmallFacePolicy::Review));
        assert_eq!(c.category, FaceCategory::Match);
        assert_eq!(c.person_id, Some(2));
        assert_eq!(c.decision, Decision::Blur);
        assert_eq!(c.pile, Pile::Matches);
    }

    #[test]
    fn uncertain_match_waits_for_a_person() {
        let e = l2_normalize(vec![0.4, 0.0, 1.0]);
        let c = classify(80.0, Some(&e), &roster(), &cfg(SmallFacePolicy::Ignore));
        assert_eq!(c.category, FaceCategory::Uncertain);
        assert_eq!(c.decision, Decision::Pending);
        assert_eq!(c.pile, Pile::Check);
    }

    #[test]
    fn small_confident_face_is_never_auto_matched() {
        let e = l2_normalize(vec![1.0, 0.0, 0.0]);
        let c = classify(20.0, Some(&e), &roster(), &cfg(SmallFacePolicy::Ignore));
        assert_eq!(c.category, FaceCategory::Uncertain);
        assert_eq!(c.decision, Decision::Pending);
    }

    #[test]
    fn small_unknown_faces_follow_policy() {
        let e = l2_normalize(vec![0.0, 0.0, 1.0]);
        let r = roster();
        let review = classify(20.0, Some(&e), &r, &cfg(SmallFacePolicy::Review));
        assert_eq!(
            (review.category, review.pile, review.decision),
            (FaceCategory::Small, Pile::Check, Decision::Pending)
        );
        let blur = classify(20.0, None, &r, &cfg(SmallFacePolicy::Blur));
        assert_eq!((blur.pile, blur.decision), (Pile::None, Decision::Blur));
        let ignore = classify(20.0, None, &r, &cfg(SmallFacePolicy::Ignore));
        assert_eq!((ignore.pile, ignore.decision), (Pile::None, Decision::Keep));
    }

    #[test]
    fn strangers_are_left_alone() {
        let e = l2_normalize(vec![0.0, 0.0, 1.0]);
        let c = classify(90.0, Some(&e), &roster(), &cfg(SmallFacePolicy::Review));
        assert_eq!((c.category, c.decision, c.pile), (FaceCategory::None, Decision::Keep, Pile::None));
        let empty = classify(90.0, Some(&e), &RosterIndex::default(), &cfg(SmallFacePolicy::Review));
        assert_eq!(empty.category, FaceCategory::None);
    }
}
