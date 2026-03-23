#![allow(dead_code)]
#![allow(unused_imports)]

use hegel::TestCase;
use hegel::generators::{integers, text};
use hegel::stateful::{Variables, variables};
use std::collections::HashMap;

const LIMIT: i64 = 1000000;

struct Ledger {
    balances: HashMap<String, i64>,
}

impl Ledger {
    fn credit(&mut self, account: String, amount: i64) {
        let balance = self.balances.entry(account).or_insert(0);
        *balance += amount;
    }

    fn debit(&mut self, account: String, amount: i64) {
        let balance = self.balances.entry(account).or_insert(0);
        *balance -= amount;
    }

    // Something here isn't quite right...
    fn transfer(&mut self, from: String, to: String, amount: i64) {
        let from_balance = *self.balances.get(&from).unwrap_or(&0);
        if from != to && amount - from_balance <= 1 {
            self.debit(from, amount);
            self.credit(to, amount);
        }
    }

    fn new() -> Ledger {
        Ledger {
            balances: HashMap::new(),
        }
    }
}

struct LedgerTest {
    ledger: Ledger,
    accounts: Variables<String>,
}

#[hegel::state_machine]
impl LedgerTest {
    #[rule]
    fn create_account(&mut self, tc: &TestCase) {
        let account = tc.draw(text().min_size(1));
        tc.note(&format!("create account '{}'", account.clone()));
        self.accounts.add(account);
    }

    #[rule]
    fn credit(&mut self, tc: &TestCase) {
        let account = self.accounts.draw().clone();
        let amount = tc.draw(integers::<i64>().min_value(0).max_value(LIMIT));
        tc.note(&format!("credit '{}' with {}", account.clone(), amount));
        self.ledger.credit(account, amount);
    }

    #[rule]
    fn transfer(&mut self, tc: &TestCase) {
        let from = self.accounts.draw().clone();
        let to = self.accounts.draw().clone();
        let amount = tc.draw(integers::<i64>().min_value(0).max_value(LIMIT));
        tc.note(&format!(
            "transfer '{}' from {} to {}",
            amount,
            from.clone(),
            to.clone()
        ));
        self.ledger.transfer(from, to, amount);
    }

    #[invariant]
    fn nonnegative_balances(&self, _: &TestCase) {
        for (_account, balance) in &self.ledger.balances {
            assert!(*balance >= 0);
        }
    }
}

#[hegel::test]
fn test_ledger(tc: TestCase) {
    let test = LedgerTest {
        ledger: Ledger::new(),
        accounts: variables(&tc),
    };
    hegel::stateful::run(test, tc);
}

fn main() {}
