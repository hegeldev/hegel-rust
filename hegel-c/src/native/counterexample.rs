//! The engine's representation of a failing test case.
//!
//! A [`Counterexample`] is everything the engine knows about one failure
//! origin (a panic site, `file:line:col`) during a run:
//!
//! - the **incumbent**, the best failing execution it holds — the shrinker's
//!   subject, the reported example, and the first stored timeline;
//! - the **pool**, other realized executions of the same failure captured
//!   when it was confirmed or reproduced, replayed when the incumbent alone
//!   does not reproduce (decision 25);
//! - its [`Standing`] — how far the failure is believed — and the replay
//!   evidence behind that belief, which the report's caveat quotes
//!   (decision 3);
//! - the pre-flip **history** a late nondeterminism detection backtracks over
//!   (decision 66);
//! - the per-run **budgets** the repeated statistical tests spend
//!   (decision 72).
//!
//! One value per origin lives in `Engine::origins`. The stored form of a
//! counterexample under nondeterministic handling — a version-2 database
//! entry or an ND reproduce blob — is [`NdReproState`], built by
//! [`Counterexample::repro_state`]; a deterministic run stores the
//! incumbent's values alone, as it always has.
//!
//! Two rules of the design are enforced here rather than at call sites.
//! Confirmation gates admission (decisions 20, 21, 24): a raw interesting
//! execution founds or shortlex-displaces an incumbent through
//! [`Counterexample::adopt`], which the engine only calls while the run is
//! deterministic or the origin is vacant; a rejection evicts the incumbent
//! without forgetting the origin, so a re-sighting resumes against the same
//! evidence and budgets and the caveat-only report can quote what was
//! measured. And the stored pool has three writers: [`Counterexample::confirm`]
//! and [`Counterexample::trust`] set it, truncated to [`nd::POOL_CAP`];
//! [`Counterexample::capture`] appends a failing execution no stored
//! timeline described; [`Counterexample::install_set`] is the multiverse
//! passes' result; and [`Counterexample::timelines`] composes the pool
//! with the current incumbent.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;

use crate::control::{InternalError, hegel_internal_unwrap};
use crate::native::HashSet;
use crate::native::blob::NdReproState;
use crate::native::core::{ChoiceNode, ChoiceValue, sort_key};
use crate::native::database::{serialize_choices, serialize_nodes};
use crate::native::nd::{self, Evidence, GauntletSpend};
use crate::native::test_runner::RunResult;

/// How far a counterexample is believed. The only transitions are the ones
/// [`Counterexample`]'s methods implement: `Unconfirmed → Confirmed` is a
/// discovery-bar accept (the post-generation sweep, shrink admission, a
/// backtrack, or the final replay's pooled review handing its reproducing
/// run to a fresh batch — decision 72); `Unconfirmed → Trusted` is
/// database or blob reproduction; `Trusted → Confirmed` is promotion by a
/// failing evidence batch. Rejection never demotes and never removes state.
#[derive(Default)]
pub(crate) enum Standing {
    /// Observed interesting; hasn't passed the discovery bar. A
    /// deterministic run's failures stay here — the standing only matters
    /// once the run is under nondeterministic handling.
    #[default]
    Unconfirmed,
    /// Reproduced from stored state: exempt from the bar's verdict and from
    /// eviction (decision 24). Carries the stored pool but no anchor until
    /// an evidence batch promotes it.
    Trusted,
    /// Past the discovery bar, or promoted from `Trusted` by a failing
    /// evidence batch.
    Confirmed {
        /// Failure-rate anchor the gauntlet prices shrink candidates
        /// against; monotone, raised only by validated accepts
        /// (decision 19).
        anchor: f64,
        /// The confirmation run the shrinker starts from; taken once.
        witness: Option<RunResult>,
    },
}

/// One pre-flip interesting execution retained for the backtrack scan.
pub(crate) struct HistoryEntry {
    pub(crate) nodes: Vec<ChoiceNode>,
    /// Whether this entry became the incumbent when recorded (founding
    /// sighting or shortlex displacement). Accepts strictly shrink, so the
    /// accept entries form the shortlex-sorted segment the scan probes
    /// geometrically; the rest are raw sightings, probed individually.
    pub(crate) accept: bool,
}

/// Everything a never-confirmed origin failed with before any flip: raw
/// sightings and shrink accepts alike, in execution order, deduplicated by
/// serialized choices, unbounded (gate G24: a recency bound evicts exactly
/// the entries an early slip-in needs). A late flip backtracks over these
/// to find the reproduction boundary. Dropped when the origin confirms —
/// the pool takes over — which also keeps the accept segment sorted: no
/// post-restore accept is ever recorded.
#[derive(Default)]
pub(crate) struct History {
    entries: Vec<HistoryEntry>,
    seen: HashSet<Vec<u8>>,
}

impl History {
    fn record(&mut self, nodes: &[ChoiceNode], accept: bool) -> Result<(), InternalError> {
        let key = hegel_internal_unwrap!(
            serialize_nodes(nodes),
            "an executed test case's clone values nest deeper than MAX_CLONE_DEPTH"
        );
        if self.seen.insert(key) {
            self.entries.push(HistoryEntry {
                nodes: nodes.to_vec(),
                accept,
            });
        }
        Ok(())
    }

    pub(crate) fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The stored-timeline set for one counterexample: the incumbent first,
/// then deduplicated pool entries, capped at [`nd::POOL_CAP`] timelines in
/// total, incumbent included. Every pool the engine stores, persists, or
/// replays is built here, so the cap comparison is written once.
pub(crate) fn pooled_timelines(
    incumbent: Vec<ChoiceValue>,
    rest: impl IntoIterator<Item = Vec<ChoiceValue>>,
) -> Vec<Vec<ChoiceValue>> {
    let mut timelines = Vec::from([incumbent]);
    for timeline in rest {
        if timelines.len() < nd::POOL_CAP && !timelines.contains(&timeline) {
            timelines.push(timeline);
        }
    }
    timelines
}

/// A failing test case, as the engine holds it for one origin. See the
/// module docs.
#[derive(Default)]
pub(crate) struct Counterexample {
    /// The best failing execution known — with its constraints, because the
    /// shrinker works on nodes. `None` once a discovery-bar rejection
    /// evicted it: the origin keeps its standing, evidence, history, and
    /// budgets, so a re-sighting resumes where it left off and the
    /// caveat-only report can quote what was measured.
    incumbent: Option<Vec<ChoiceNode>>,
    /// The timelines captured when the origin was confirmed or trusted, the
    /// confirm-time incumbent first, truncated to [`nd::POOL_CAP`]. Kept as
    /// captured: after shrinking moves the incumbent, the confirm-time
    /// example stays here as a replay fallback. [`Self::timelines`] puts the
    /// current incumbent ahead of it.
    pool: Vec<Vec<ChoiceValue>>,
    standing: Standing,
    /// Physical replay evidence behind the standing: confirmation batches,
    /// reuse reproductions, and shrink-time batches (report-time replays
    /// are kept apart).
    fails: u64,
    replays: u64,
    /// The report-time final replay's evidence, quoted separately by the
    /// caveat; a zero-fail record switches its wording to
    /// dry-at-report-time.
    report_fails: u64,
    report_replays: u64,
    /// Starting evidence for the next evidence batch, deposited by the
    /// first-interesting check's replays (seam plan step 2) so the discovery
    /// bar begins partially filled. Taken once.
    seed: Option<Evidence>,
    history: History,
    /// Whether the first-interesting determinism check has run for this
    /// origin (either verdict — a miss flips the run, which handles
    /// everything after), or was waived: database-reuse reproductions were
    /// already replayed once.
    first_checked: bool,
    /// Bar batches spent this run by the sweep, shrink admission, and the
    /// pooled review, capped at [`nd::BAR_ATTEMPTS_PER_RUN`] (decision 72).
    bar_attempts: u64,
    /// Bar batches spent across this origin's backtracks, capped at
    /// [`nd::BACKTRACK_BAR_ATTEMPTS`] — a budget separate from
    /// `bar_attempts` because backtrack candidates come from history.
    backtrack_attempts: u64,
    /// Gauntlet alpha-spending state (decision 72). Held here rather than
    /// on the shrink probe so a re-shrink's rebuilt probe keeps spending
    /// from the same budget.
    pub(crate) gauntlet_spend: GauntletSpend,
    /// How often the incumbent's own measurement replays left it — did not
    /// stay live on the first timeline — out of how many (decision 75): the
    /// rate a shrink candidate's bounce budget is derived from.
    bounces: u64,
    bounce_runs: u64,
}

/// The order of two timelines within a counterexample (decision 74): the
/// database's shortlex over serialized values — fewer flattened choices
/// first, then the bytes. Timelines that cannot be serialized compare equal.
pub(crate) fn timeline_order(a: &[ChoiceValue], b: &[ChoiceValue]) -> Ordering {
    match crate::native::core::flattened_values_len(a)
        .cmp(&crate::native::core::flattened_values_len(b))
    {
        Ordering::Equal => {}
        ord => return ord,
    }
    match (serialize_choices(a), serialize_choices(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        _ => Ordering::Equal,
    }
}

/// The order of two counterexamples (decision 74): fewer timelines first,
/// then the timelines lexicographically under [`timeline_order`].
pub(crate) fn set_order(a: &[Vec<ChoiceValue>], b: &[Vec<ChoiceValue>]) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {}
        ord => return ord,
    }
    for (x, y) in a.iter().zip(b) {
        match timeline_order(x, y) {
            Ordering::Equal => {}
            ord => return ord,
        }
    }
    Ordering::Equal
}

impl Counterexample {
    /// The confirmation anchor, once confirmed.
    pub(crate) fn anchor(&self) -> Option<f64> {
        match self.standing {
            Standing::Confirmed { anchor, .. } => Some(anchor),
            _ => None,
        }
    }

    /// Fold an incumbent measurement batch's bounces — replays that did not
    /// stay live on the incumbent — into the origin's record.
    pub(crate) fn record_bounces(&mut self, bounces: u64, runs: u64) {
        self.bounces += bounces;
        self.bounce_runs += runs;
    }

    /// The incumbent's (bounces, runs) so far.
    pub(crate) fn bounce_stats(&self) -> (u64, u64) {
        (self.bounces, self.bounce_runs)
    }

    /// Capture a realized failing execution the counterexample did not
    /// describe — a measurement replay that failed while live on no stored
    /// timeline (decision 75): a branch the failure takes that the pool
    /// lacked. Appended last, so it serves only where every earlier
    /// timeline disagrees; the census drops it again if it never serves.
    /// Bounded by [`nd::POOL_CAP`] with the incumbent.
    pub(crate) fn capture(&mut self, timeline: Vec<ChoiceValue>) -> bool {
        if self.pool.len() + 1 >= nd::POOL_CAP
            || self.pool.contains(&timeline)
            || self.incumbent_values().is_some_and(|i| i == timeline)
        {
            return false;
        }
        self.pool.push(timeline);
        true
    }

    /// Install a structurally shrunk counterexample (decision 74's
    /// multiverse passes): `set[0]` becomes the incumbent — as `nodes` when
    /// it changed, which must be a run that stayed live on it — and the
    /// rest becomes the pool, in order.
    pub(crate) fn install_set(&mut self, set: &[Vec<ChoiceValue>], nodes: Option<Vec<ChoiceNode>>) {
        if let Some(nodes) = nodes {
            self.incumbent = Some(nodes);
        }
        self.pool = set.iter().skip(1).cloned().collect();
    }
    /// The best failing execution held, if any.
    pub(crate) fn incumbent(&self) -> Option<&[ChoiceNode]> {
        self.incumbent.as_deref()
    }

    /// The incumbent's realized values — the deterministic stored form and
    /// the first stored timeline.
    pub(crate) fn incumbent_values(&self) -> Option<Vec<ChoiceValue>> {
        self.incumbent
            .as_ref()
            .map(|nodes| nodes.iter().map(|n| n.value()).collect())
    }

    /// A raw interesting execution: found the incumbent if the origin is
    /// vacant, or displace it if `nodes` shortlex-precedes it. Returns
    /// whether `nodes` became the incumbent.
    pub(crate) fn adopt(&mut self, nodes: Vec<ChoiceNode>) -> bool {
        match &self.incumbent {
            Some(current) if sort_key(&nodes) >= sort_key(current) => false,
            _ => {
                self.incumbent = Some(nodes);
                true
            }
        }
    }

    /// Install `nodes` as the incumbent unconditionally — a shrink result,
    /// a backtrack restore, or a flip that returns a deterministic shrink's
    /// starting point.
    pub(crate) fn replace(&mut self, nodes: Vec<ChoiceNode>) {
        self.incumbent = Some(nodes);
    }

    /// Remove the incumbent, keeping everything else known about the
    /// origin. Returns what was evicted.
    pub(crate) fn evict(&mut self) -> Option<Vec<ChoiceNode>> {
        self.incumbent.take()
    }

    /// The captured timeline pool; empty unless confirmed or trusted from a
    /// v2 entry.
    pub(crate) fn pool(&self) -> &[Vec<ChoiceValue>] {
        &self.pool
    }

    /// The timelines to replay for this counterexample with `incumbent` in
    /// front of the captured pool: see [`pooled_timelines`].
    pub(crate) fn timelines_from(&self, incumbent: Vec<ChoiceValue>) -> Vec<Vec<ChoiceValue>> {
        pooled_timelines(incumbent, self.pool.iter().cloned())
    }

    /// The timelines to replay for this counterexample: the current
    /// incumbent's values first, then the captured pool. Empty with no
    /// incumbent.
    pub(crate) fn timelines(&self) -> Vec<Vec<ChoiceValue>> {
        match self.incumbent_values() {
            Some(incumbent) => self.timelines_from(incumbent),
            None => Vec::new(),
        }
    }

    /// The stored form under nondeterministic handling for `incumbent` (the
    /// current one, or a candidate the shrinker has adopted but not yet
    /// handed back): the timelines incumbent-first, content-hash entropy
    /// (so identical state re-encodes identically across runs), and the
    /// standard continuation extension.
    pub(crate) fn repro_state(
        &self,
        incumbent: Vec<ChoiceValue>,
    ) -> Result<NdReproState, InternalError> {
        let len = crate::native::core::flattened_values_len(&incumbent);
        let timelines = self.timelines_from(incumbent);
        let mut content = Vec::new();
        for timeline in &timelines {
            let bytes = hegel_internal_unwrap!(
                serialize_choices(timeline),
                "a stored timeline's clone values nest deeper than MAX_CLONE_DEPTH"
            );
            content.extend_from_slice(&bytes);
        }
        Ok(NdReproState {
            timelines,
            entropy: crate::native::database::fnv1a(&content),
            extension: (nd::continuation_budget(len) - len) as u32,
        })
    }

    /// Whether the origin still has to face the discovery bar before its
    /// replay state is usable.
    pub(crate) fn needs_confirmation(&self) -> bool {
        matches!(self.standing, Standing::Unconfirmed)
    }

    /// Stored state reproduced this origin: trusted without re-running the
    /// bar (decision 24). `pool` carries the reproducing entry's stored
    /// timelines (empty for v1 entries), truncated to [`nd::POOL_CAP`] — a
    /// decoded entry can carry up to the looser format bound. `evidence`
    /// is the reproducing replay batch's physical (fails, replays), folded
    /// into the trusted counts. Never demotes `Confirmed`, and never
    /// replaces an existing pool with an empty one.
    pub(crate) fn trust(&mut self, mut pool: Vec<Vec<ChoiceValue>>, evidence: (u64, u64)) {
        pool.truncate(nd::POOL_CAP);
        match self.standing {
            Standing::Confirmed { .. } => {}
            Standing::Trusted => {
                if !pool.is_empty() {
                    self.pool = pool;
                }
                self.fails += evidence.0;
                self.replays += evidence.1;
            }
            Standing::Unconfirmed => {
                self.standing = Standing::Trusted;
                self.pool = pool;
                self.fails = evidence.0;
                self.replays = evidence.1;
                self.report_fails = 0;
                self.report_replays = 0;
            }
        }
    }

    /// A shrink-time evidence batch on a trusted origin produced no
    /// failure: fold its physical (fails, replays) into the trusted counts
    /// — the origin stays `Trusted`, skips shrinking, and is still
    /// reported and persisted. No-op in any other standing.
    pub(crate) fn record_trusted_batch(&mut self, evidence: (u64, u64)) {
        if matches!(self.standing, Standing::Trusted) {
            self.fails += evidence.0;
            self.replays += evidence.1;
        }
    }

    /// The discovery bar accepted this origin, or a failing evidence batch
    /// promoted it from `Trusted`: store its replay state and drop the
    /// pre-flip history, whose job the pool takes over. `evidence` is the
    /// batch's physical (fails, replays), folded into the cumulative
    /// counts. The pool is truncated to [`nd::POOL_CAP`]. Confirming a
    /// confirmed origin is a violated invariant: every caller sits behind
    /// a `needs_confirmation`/`take_witness` check.
    pub(crate) fn confirm(
        &mut self,
        anchor: f64,
        witness: Option<RunResult>,
        mut pool: Vec<Vec<ChoiceValue>>,
        evidence: (u64, u64),
    ) -> Result<(), crate::control::InternalError> {
        if matches!(self.standing, Standing::Confirmed { .. }) {
            crate::control::hegel_internal_error!(
                "Counterexample::confirm: the origin is already confirmed"
            );
        }
        pool.truncate(nd::POOL_CAP);
        self.standing = Standing::Confirmed { anchor, witness };
        self.pool = pool;
        self.fails += evidence.0;
        self.replays += evidence.1;
        self.history = History::default();
        Ok(())
    }

    /// A validated accept measured the failure rate at `anchor` (a gauntlet
    /// first-accept or a boost holdout). Monotone: never lowers the stored
    /// anchor (decision 19), and a no-op unless confirmed — an anchor only
    /// exists past the bar.
    pub(crate) fn raise_anchor(&mut self, anchor: f64) {
        if let Standing::Confirmed { anchor: stored, .. } = &mut self.standing {
            if anchor > *stored {
                *stored = anchor;
            }
        }
    }

    /// Take the stored witness run and its anchor as the shrinker's
    /// starting point. Yields once per confirmation.
    pub(crate) fn take_witness(&mut self) -> Option<(RunResult, f64)> {
        match &mut self.standing {
            Standing::Confirmed { anchor, witness } => witness.take().map(|w| (w, *anchor)),
            _ => None,
        }
    }

    /// Record the report-time final replay's physical (fails, replays),
    /// kept apart from the confirmation or reuse evidence. No-op unless
    /// trusted or confirmed — no other standing reaches the final replay.
    pub(crate) fn record_final_replay(&mut self, evidence: (u64, u64)) {
        if !self.needs_confirmation() {
            self.report_fails += evidence.0;
            self.report_replays += evidence.1;
        }
    }

    /// The discovery bar rejected this origin with the rejecting batch's
    /// physical (fails, replays): an unconfirmed origin records the
    /// rejection for the caveated report and loses its incumbent, which is
    /// returned; trusted and confirmed origins are exempt and keep it.
    pub(crate) fn reject(&mut self, evidence: (u64, u64)) -> Option<Vec<ChoiceNode>> {
        if !self.needs_confirmation() {
            return None;
        }
        self.fails += evidence.0;
        self.replays += evidence.1;
        self.evict()
    }

    /// Deposit the first-interesting check's replay observations as the
    /// next evidence batch's starting point. Counts no rejection — the
    /// check is detection, not a bar verdict.
    pub(crate) fn seed_evidence(&mut self, evidence: Evidence) {
        self.seed = Some(evidence);
    }

    /// The seeded starting evidence, taken at most once.
    pub(crate) fn take_seed(&mut self) -> Option<Evidence> {
        self.seed.take()
    }

    /// Append a pre-flip interesting execution to the history (see
    /// [`History`]). `accept` says whether it became the incumbent.
    pub(crate) fn record_sighting(
        &mut self,
        nodes: &[ChoiceNode],
        accept: bool,
    ) -> Result<(), InternalError> {
        self.history.record(nodes, accept)
    }

    pub(crate) fn history(&self) -> &History {
        &self.history
    }

    /// Drop the history without confirming (test seam: a backtrack with
    /// nothing to scan).
    #[cfg(test)]
    pub(crate) fn clear_history(&mut self) {
        self.history = History::default();
    }

    pub(crate) fn first_checked(&self) -> bool {
        self.first_checked
    }

    /// Mark the first-interesting check done (or waived) for this origin.
    pub(crate) fn mark_first_checked(&mut self) {
        self.first_checked = true;
    }

    /// Spend one bar attempt (decision 72). False once the budget is gone:
    /// the caller treats the origin as a bar reject without running a
    /// batch.
    pub(crate) fn spend_bar_attempt(&mut self) -> bool {
        if self.bar_attempts >= nd::BAR_ATTEMPTS_PER_RUN {
            return false;
        }
        self.bar_attempts += 1;
        true
    }

    /// Whether backtrack bar attempts remain, without spending one —
    /// checked before a backtrack pays for its history scan.
    pub(crate) fn backtrack_attempts_left(&self) -> bool {
        self.backtrack_attempts < nd::BACKTRACK_BAR_ATTEMPTS
    }

    /// Spend one backtrack bar attempt (decision 72): the budget holds
    /// across backtracks of the same origin, not per call.
    pub(crate) fn spend_backtrack_attempt(&mut self) -> bool {
        if !self.backtrack_attempts_left() {
            return false;
        }
        self.backtrack_attempts += 1;
        true
    }

    /// The caveat attached to this failure's report under nondeterministic
    /// handling (decision 3): its standing with the in-run replay
    /// evidence, worded per the standing. Quotes only in-run measurements,
    /// and weights the environment-modification hypothesis only when
    /// non-reproduction is surprising given the evidence.
    pub(crate) fn caveat(&self) -> String {
        let Counterexample {
            fails,
            replays,
            report_fails,
            report_replays,
            ..
        } = self;
        match self.standing {
            Standing::Confirmed { .. } => {
                if *report_replays > 0 && *report_fails == 0 {
                    format!(
                        "nondeterministic failure, confirmed earlier this run \
                         (failed {fails} of {replays} replays) but not reproduced \
                         at report time — a rare failure, or something in the \
                         environment changed after discovery"
                    )
                } else if *report_replays > 0 {
                    format!(
                        "nondeterministic failure, confirmed: failed {fails} of \
                         {replays} replays at confirmation and {report_fails} of \
                         {report_replays} at report time"
                    )
                } else {
                    format!(
                        "nondeterministic failure, confirmed: failed {fails} of \
                         {replays} replays this run"
                    )
                }
            }
            Standing::Trusted => {
                if *report_replays > 0 && *report_fails == 0 {
                    format!(
                        "nondeterministic failure, reproduced from stored \
                         timelines earlier this run (failed {fails} of {replays} \
                         replays) but not reproduced at report time — a rare \
                         failure, or something in the environment changed after \
                         discovery"
                    )
                } else if *report_replays > 0 {
                    format!(
                        "nondeterministic failure, reproduced from stored \
                         timelines: failed {fails} of {replays} replays at reuse \
                         and {report_fails} of {report_replays} at report time"
                    )
                } else {
                    format!(
                        "nondeterministic failure, reproduced from stored \
                         timelines: failed {fails} of {replays} replays this run"
                    )
                }
            }
            Standing::Unconfirmed => {
                if *fails > 0 {
                    format!(
                        "unconfirmed failure: failed {fails} of {replays} replays \
                         this run, below the confirmation bar — likely rare"
                    )
                } else if *replays == 0 {
                    String::from(
                        "unconfirmed failure: observed once, never replayed — a rare \
                         failure, or the environment changed between executions",
                    )
                } else {
                    format!(
                        "unconfirmed failure: failed 0 of {replays} replays after \
                         the observed failure — a rare failure, or the environment \
                         changed between executions"
                    )
                }
            }
        }
    }
}

/// The run's counterexamples by origin. A thin map with the queries the
/// engine asks across origins; per-origin state is the
/// [`Counterexample`]'s own.
#[derive(Default)]
pub(crate) struct Counterexamples {
    origins: BTreeMap<String, Counterexample>,
}

impl Counterexamples {
    pub(crate) fn get(&self, origin: &str) -> Option<&Counterexample> {
        self.origins.get(origin)
    }

    pub(crate) fn get_mut(&mut self, origin: &str) -> Option<&mut Counterexample> {
        self.origins.get_mut(origin)
    }

    /// The record for `origin`, created blank on first use: an origin
    /// exists from the moment the run learns anything about it.
    pub(crate) fn entry(&mut self, origin: &str) -> &mut Counterexample {
        self.origins.entry(String::from(origin)).or_default()
    }

    /// The incumbent held for `origin`, if any.
    pub(crate) fn incumbent(&self, origin: &str) -> Option<&[ChoiceNode]> {
        self.origins.get(origin).and_then(Counterexample::incumbent)
    }

    /// Whether `origin` still has to face the discovery bar: unknown
    /// origins do.
    pub(crate) fn needs_confirmation(&self, origin: &str) -> bool {
        self.origins
            .get(origin)
            .is_none_or(Counterexample::needs_confirmation)
    }

    /// Origins holding an incumbent — the failures the run will shrink,
    /// replay, persist, and report — in origin order, with their
    /// incumbents.
    pub(crate) fn live(&self) -> impl Iterator<Item = (&str, &[ChoiceNode])> {
        self.origins
            .iter()
            .filter_map(|(origin, c)| c.incumbent().map(|nodes| (origin.as_str(), nodes)))
    }

    /// Whether any origin holds an incumbent: the run has a failure.
    pub(crate) fn any_live(&self) -> bool {
        self.live().next().is_some()
    }

    /// The live origins, in origin order.
    pub(crate) fn live_origins(&self) -> Vec<String> {
        self.live()
            .map(|(origin, _)| String::from(origin))
            .collect()
    }

    /// Every origin known this run with its record, in origin order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &Counterexample)> {
        self.origins.iter().map(|(origin, c)| (origin.as_str(), c))
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut Counterexample)> {
        self.origins
            .iter_mut()
            .map(|(origin, c)| (origin.as_str(), c))
    }

    /// Origins observed but never confirmed — bar rejects and never-barred
    /// sightings alike — in origin order: the caveated-failure report
    /// (decision 3), used only when nothing confirmed (decision 24).
    pub(crate) fn unconfirmed(&self) -> impl Iterator<Item = &str> {
        self.origins
            .iter()
            .filter_map(|(origin, c)| c.needs_confirmation().then_some(origin.as_str()))
    }

    /// The caveat for `origin`'s report, or `None` for an origin the run
    /// never recorded.
    pub(crate) fn caveat(&self, origin: &str) -> Option<String> {
        self.origins.get(origin).map(Counterexample::caveat)
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/counterexample_tests.rs"]
mod tests;
