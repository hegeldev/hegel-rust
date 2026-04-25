//! Ported from hypothesis-python/tests/nocover/test_database_agreement.py
//!
//! State-machine test that cross-checks the three database backends —
//! `InMemoryNativeDatabase`, `NativeDatabase` (directory-based), and
//! `BackgroundWriteNativeDatabase` wrapping an in-memory store — agree on
//! `fetch` results after arbitrary save/delete/move sequences. Native-gated
//! because all three database types live under `src/native/database.rs`.

#![cfg(feature = "native")]

use hegel::TestCase;
use hegel::__native_test_internals::{
    BackgroundWriteNativeDatabase, ExampleDatabase, InMemoryNativeDatabase, NativeDatabase,
};
use hegel::generators as gs;
use hegel::stateful::{Rule, StateMachine, Variables, run as run_state_machine, variables};
use hegel::{Hegel, Settings};
use std::collections::HashSet;
use std::sync::Arc;

struct DatabaseAgreementMachine {
    dbs: Vec<Arc<dyn ExampleDatabase>>,
    keys: Variables<Vec<u8>>,
    values: Variables<Vec<u8>>,
}

impl DatabaseAgreementMachine {
    fn new(tc: &TestCase, exampledir: &str) -> Self {
        let dbs: Vec<Arc<dyn ExampleDatabase>> = vec![
            Arc::new(InMemoryNativeDatabase::new()),
            Arc::new(NativeDatabase::new(exampledir)),
            Arc::new(BackgroundWriteNativeDatabase::new(
                InMemoryNativeDatabase::new(),
            )),
        ];
        Self {
            dbs,
            keys: variables(tc),
            values: variables(tc),
        }
    }

    fn rule_k(&mut self, tc: TestCase) {
        let k: Vec<u8> = tc.draw(gs::binary());
        self.keys.add(k);
    }

    fn rule_v(&mut self, tc: TestCase) {
        let v: Vec<u8> = tc.draw(gs::binary());
        self.values.add(v);
    }

    fn rule_save(&mut self, _tc: TestCase) {
        let k = self.keys.draw();
        let v = self.values.draw();
        for db in &self.dbs {
            db.save(k, v);
        }
    }

    fn rule_delete(&mut self, _tc: TestCase) {
        let k = self.keys.draw();
        let v = self.values.draw();
        for db in &self.dbs {
            db.delete(k, v);
        }
    }

    fn rule_move(&mut self, _tc: TestCase) {
        let k1 = self.keys.draw();
        let k2 = self.keys.draw();
        let v = self.values.draw();
        for db in &self.dbs {
            db.move_value(k1, k2, v);
        }
    }

    fn rule_values_agree(&mut self, _tc: TestCase) {
        let k = self.keys.draw();
        let mut last: Option<HashSet<Vec<u8>>> = None;
        for db in &self.dbs {
            let entries: HashSet<Vec<u8>> = db.fetch(k).into_iter().collect();
            if let Some(prev) = &last {
                assert_eq!(prev, &entries);
            }
            last = Some(entries);
        }
    }
}

impl StateMachine for DatabaseAgreementMachine {
    fn rules(&self) -> Vec<Rule<Self>> {
        vec![
            Rule::new("k", Self::rule_k),
            Rule::new("v", Self::rule_v),
            Rule::new("save", Self::rule_save),
            Rule::new("delete", Self::rule_delete),
            Rule::new("move", Self::rule_move),
            Rule::new("values_agree", Self::rule_values_agree),
        ]
    }
    fn invariants(&self) -> Vec<Rule<Self>> {
        vec![]
    }
}

#[test]
fn test_database_equivalence() {
    Hegel::new(|tc: TestCase| {
        let tmp = tempfile::TempDir::new().unwrap();
        let exampledir = tmp.path().join("examples");
        let machine = DatabaseAgreementMachine::new(&tc, exampledir.to_str().unwrap());
        run_state_machine(machine, tc);
    })
    .settings(Settings::new().test_cases(20).database(None))
    .run();
}
