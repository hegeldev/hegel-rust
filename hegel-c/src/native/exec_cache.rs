//! The flat replacements for the data tree (experiment 010): a two-tier
//! execution cache and a choice-kind ledger.
//!
//! The cache keys every executed conclusion on its realized choice values —
//! [`crate::native::database::serialize_choices`] gives the exact key
//! semantics the tree's `ChoiceValueKey` had (floats by bit pattern, clones
//! by child values). The verdict tier holds a 128-bit digest of that key for
//! every conclusion and is what duplicate detection and verdict-flip
//! detection read; the full tier holds complete serving entries (status,
//! origin, nodes, spans) so shrink-phase repeats are served without
//! re-running the body, byte-bounded with oldest-first eviction. Overruns
//! enter neither tier: a proposal that ran out of data concluded nothing.
//!
//! The kind ledger is the tree's generation-nondeterminism detector: for
//! each value prefix it remembers the choice kind (constraints included)
//! drawn at the next position, and a within-run contradiction produces the
//! tree's diagnostic verbatim. Between-run drift is deliberately invisible
//! to it — a stored entry that stops reproducing is staleness, never
//! nondeterminism evidence.

use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::control::{InternalError, hegel_internal_unwrap};
use crate::native::HashMap;
use crate::native::core::{ChoiceKind, ChoiceNode, Span, Status};

/// Byte budget for the full serving tier. Entries are charged their key
/// bytes plus the shallow size of their nodes and spans; the oldest entries
/// are evicted first. Sized so a shrink phase's working set (thousands of
/// probes of a ~50-node case) fits with a wide margin.
const FULL_TIER_MAX_BYTES: usize = 8 << 20;

/// Entry cap for the kind ledger. Once full it stops learning new positions
/// but keeps checking known ones; detection degrades, correctness doesn't.
const KIND_LEDGER_CAP: usize = 1 << 16;

/// 128-bit FNV-1a, the digest the verdict tier and the ledger key on. Collisions
/// would surface as a false duplicate or a false kind contradiction, so the
/// digest is sized to make them negligible rather than merely rare.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Digest(u128);

const FNV_OFFSET: u128 = 0x6c62272e07bb014262b821756295c58d;
const FNV_PRIME: u128 = 0x0000000001000000000000000000013b;

impl Digest {
    fn new() -> Self {
        Digest(FNV_OFFSET)
    }

    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u128::from(b);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    pub(crate) fn of(bytes: &[u8]) -> Self {
        let mut d = Digest::new();
        d.update(bytes);
        d
    }
}

/// A conclusion's verdict, compared on a digest hit: the same realized
/// values concluding with a different status or origin is the flake the
/// tree could never see.
struct Verdict {
    status: Status,
    origin: Option<String>,
}

/// One full serving entry — everything [`cached_test_function`]'s consumers
/// read off a run: the shrinker takes status, origin, nodes, and spans;
/// span mutation reads status only. Target observations are not stored (no
/// consumer reads them off a served run) and events are execution-only.
///
/// [`cached_test_function`]: crate::native::test_runner::Engine::cached_test_function
pub(crate) struct CachedRun {
    pub(crate) status: Status,
    pub(crate) origin: Option<String>,
    pub(crate) nodes: Vec<ChoiceNode>,
    pub(crate) spans: Vec<Span>,
}

impl CachedRun {
    fn cost(&self, key_len: usize) -> usize {
        key_len
            + self.nodes.len() * core::mem::size_of::<ChoiceNode>()
            + self.spans.len() * core::mem::size_of::<Span>()
            + core::mem::size_of::<CachedRun>()
    }
}

/// What [`ExecCache::record`] observed about one conclusion.
pub(crate) struct Recorded {
    /// The fingerprint had been executed before.
    pub(crate) duplicate: bool,
    /// It had, and concluded with a different status or origin.
    pub(crate) verdict_mismatch: bool,
    /// On a verdict mismatch, the failure the two conclusions disagree
    /// about: the origin of whichever of them was interesting.
    pub(crate) mismatched_origin: Option<String>,
}

#[derive(Default)]
pub(crate) struct ExecCache {
    verdicts: HashMap<Digest, Verdict>,
    full: HashMap<Vec<u8>, CachedRun>,
    full_order: VecDeque<Vec<u8>>,
    full_bytes: usize,
    max_full_bytes: Option<usize>,
}

impl ExecCache {
    #[cfg(test)]
    fn with_full_tier_bound(max_bytes: usize) -> Self {
        ExecCache {
            max_full_bytes: Some(max_bytes),
            ..ExecCache::default()
        }
    }

    /// Record one executed conclusion under its serialized-values `key`,
    /// keeping a full serving entry only when `keep_full` (the generation
    /// phase keeps digests only: its duplicates must execute — they are the
    /// duplicate-stop signal — so serving entries would go unread).
    pub(crate) fn record(
        &mut self,
        key: Vec<u8>,
        status: Status,
        origin: Option<&str>,
        nodes: &[ChoiceNode],
        spans: &[Span],
        keep_full: bool,
    ) -> Recorded {
        let digest = Digest::of(&key);
        let recorded = match self.verdicts.get(&digest) {
            Some(v) => Recorded {
                duplicate: true,
                verdict_mismatch: v.status != status || v.origin.as_deref() != origin,
                mismatched_origin: v.origin.clone().or_else(|| origin.map(String::from)),
            },
            None => {
                self.verdicts.insert(
                    digest,
                    Verdict {
                        status,
                        origin: origin.map(String::from),
                    },
                );
                Recorded {
                    duplicate: false,
                    verdict_mismatch: false,
                    mismatched_origin: None,
                }
            }
        };
        if keep_full && !recorded.verdict_mismatch && !self.full.contains_key(&key) {
            let entry = CachedRun {
                status,
                origin: origin.map(String::from),
                nodes: nodes.to_vec(),
                spans: spans.to_vec(),
            };
            self.full_bytes += entry.cost(key.len());
            self.full_order.push_back(key.clone());
            self.full.insert(key, entry);
            let bound = self.max_full_bytes.unwrap_or(FULL_TIER_MAX_BYTES);
            while self.full_bytes > bound && !self.full_order.is_empty() {
                let oldest = self.full_order.pop_front().unwrap_or_default();
                if let Some(evicted) = self.full.remove(&oldest) {
                    self.full_bytes -= evicted.cost(oldest.len());
                }
            }
        }
        recorded
    }

    /// The full entry for `key`, if one survives: an exact repeat of an
    /// executed conclusion, servable without running the body.
    pub(crate) fn serve(&self, key: &[u8]) -> Option<CachedRun> {
        self.full.get(key).map(|e| CachedRun {
            status: e.status,
            origin: e.origin.clone(),
            nodes: e.nodes.clone(),
            spans: e.spans.clone(),
        })
    }

    /// Drop everything — both tiers. Called at the concurrent-state-machine
    /// nondeterministic flip: post-flip, identical timelines need not
    /// conclude identically, so nothing recorded pre-flip may be served or
    /// compared again.
    pub(crate) fn clear(&mut self) {
        self.verdicts.clear();
        self.full.clear();
        self.full_order.clear();
        self.full_bytes = 0;
    }
}

/// The per-prefix kind record: for each 128-bit rolling hash of a serialized
/// value prefix, the [`ChoiceKind`] (constraints included) drawn at the next
/// position. Within one run, two executions disagreeing at a shared prefix
/// is generation nondeterminism, reported with the tree's wording.
#[derive(Default)]
pub(crate) struct KindLedger {
    entries: HashMap<Digest, ChoiceKind>,
}

impl KindLedger {
    /// Fold one execution's realized nodes into the ledger, returning the
    /// tree's kind-mismatch diagnostic on the first contradiction. The engine
    /// bounds clone nesting at `MAX_CLONE_DEPTH` as a case runs, so the
    /// serializer refusing an executed node is a violated internal invariant.
    pub(crate) fn observe(
        &mut self,
        nodes: &[ChoiceNode],
    ) -> Result<Option<String>, InternalError> {
        let mut prefix = Digest::new();
        let mut scratch = Vec::new();
        for node in nodes {
            let kind = node.kind();
            match self.entries.get(&prefix) {
                Some(expected) if *expected != kind => {
                    return Ok(Some(format!(
                        "Your data generation is non-deterministic: at the same choice \
                         position with the same prefix, the choice kind changed from {:?} to {:?}. \
                         This usually means a generator depends on global mutable state.",
                        expected, kind
                    )));
                }
                None if self.entries.len() < KIND_LEDGER_CAP => {
                    self.entries.insert(prefix, kind);
                }
                _ => {}
            }
            scratch.clear();
            hegel_internal_unwrap!(
                crate::native::database::serialize_one_choice(&mut scratch, node.data.value_ref()),
                "an executed test case's clone values nest deeper than MAX_CLONE_DEPTH"
            );
            prefix.update(&scratch);
        }
        Ok(None)
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/exec_cache_tests.rs"]
mod tests;
