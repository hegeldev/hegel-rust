//! Nested shrinking of cloned streams.
//!
//! The flat passes treat a clone node as an opaque unit: they can delete it
//! or replace it with the empty clone, but never touch the choices *inside*
//! it. This pass gives each clone node its own nested [`Shrinker`] over the
//! child stream: every nested candidate is spliced into the current parent
//! sequence at the clone's position and replayed through the outer test
//! function, and the realized child stream is read back out of the run's
//! clone node. Clones nested inside the child recurse through the same pass
//! of the nested shrinker.

use std::sync::Arc;

use crate::native::core::{ChoiceNode, ChoiceValue, CloneRecord, Spans, flattened_values_len};

use super::{ShrinkResult, ShrinkRun, Shrinker};

/// `template` with the clone node at `i` carrying `child` as its stream.
/// The spliced record has no span info — replay recreates spans — and
/// carries the candidate's nodes so replay puns against the child kinds.
fn splice_child(template: &[ChoiceNode], i: usize, child: &[ChoiceNode]) -> Vec<ChoiceNode> {
    let mut candidate = template.to_vec();
    candidate[i] = candidate[i].with_value(ChoiceValue::Clone(Arc::new(CloneRecord::from_run(
        child.to_vec(),
        Vec::new(),
        Vec::new(),
    ))));
    candidate
}

impl<'a> Shrinker<'a> {
    /// Run a full nested shrink over the stream inside each clone node of
    /// the current best sequence.
    pub(super) fn shrink_clone_streams(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            self.shrink_clone_stream_at(i)?;
            i += 1;
        }
        Ok(())
    }

    /// Nested-shrink the clone node at `i`, if the node is one (with a
    /// non-empty realized stream); a no-op otherwise.
    fn shrink_clone_stream_at(&mut self, i: usize) -> ShrinkResult<()> {
        let (child_nodes, child_spans) = {
            let ChoiceValue::Clone(record) = &self.current_nodes[i].value else {
                return Ok(());
            };
            let Some(nodes) = record.realized_nodes() else {
                return Ok(());
            };
            if nodes.is_empty() {
                return Ok(());
            }
            (nodes.to_vec(), record.spans().to_vec())
        };
        let template = self.current_nodes.clone();
        let outer_values: Vec<ChoiceValue> = template.iter().map(|n| n.value.clone()).collect();
        let deadline = self.deadline;

        let (final_child, nested_timed_out) = {
            let test_fn = &mut self.test_fn;
            let mut nested = Shrinker::with_probe(
                Box::new(|req: ShrinkRun| {
                    let (matched, actual) = match req {
                        ShrinkRun::Full(child) => {
                            let candidate = splice_child(&template, i, child);
                            let (matched, actual, _) = (test_fn)(ShrinkRun::Full(&candidate));
                            (matched, actual)
                        }
                        ShrinkRun::Probe { prefix, max_size } => {
                            let mut values = outer_values.clone();
                            values[i] = ChoiceValue::Clone(Arc::new(CloneRecord::from_values(
                                prefix.to_vec(),
                            )));
                            let child_extend = max_size.saturating_sub(prefix.len());
                            let (matched, actual, _) = (test_fn)(ShrinkRun::Probe {
                                max_size: flattened_values_len(&values) + child_extend,
                                prefix: &values,
                            });
                            (matched, actual)
                        }
                    };
                    match actual.get(i).map(|n| &n.value) {
                        Some(ChoiceValue::Clone(record)) if record.realized_nodes().is_some() => {
                            let nodes = record
                                .realized_nodes()
                                .unwrap_or_else(|| unreachable!("guarded by the match arm"))
                                .to_vec();
                            let spans = Spans::from(record.spans().to_vec());
                            (matched, nodes, spans)
                        }
                        _ => (false, Vec::new(), Spans::new()),
                    }
                }),
                child_nodes,
                Spans::from(child_spans),
            );
            nested.deadline = deadline;
            nested.shrink();
            (nested.current_nodes, nested.timed_out)
        };
        self.timed_out |= nested_timed_out;

        let spliced = splice_child(&self.current_nodes, i, &final_child);
        self.consider(&spliced)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_clones_tests.rs"]
mod tests;
