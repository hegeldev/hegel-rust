use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::*;

#[test]
fn lock_gives_mutable_access_to_the_value() {
    let mutex = Mutex::new(1);
    *mutex.lock() += 1;
    assert_eq!(*mutex.lock(), 2);
}

#[test]
fn try_lock_reports_contention_and_recovers() {
    let mutex = Mutex::new(0);
    let held = mutex.lock();
    assert!(mutex.try_lock().is_none());
    drop(held);
    assert!(mutex.try_lock().is_some());
}

#[test]
fn a_second_try_lock_after_the_first_still_fails() {
    let mutex = Mutex::new(0);
    let held = mutex.try_lock().unwrap();
    assert!(mutex.try_lock().is_none());
    drop(held);
}

#[test]
fn lock_blocks_until_the_holder_releases() {
    let mutex = Mutex::new(0);
    let mutex = &mutex;
    thread::scope(|scope| {
        let (locked_tx, locked_rx) = mpsc::channel();
        scope.spawn(move || {
            let mut held = mutex.lock();
            locked_tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(50));
            *held = 7;
        });
        locked_rx.recv().unwrap();
        assert_eq!(*mutex.lock(), 7);
    });
}

#[test]
fn many_threads_serialise_their_increments() {
    let mutex = Mutex::new(0usize);
    thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                for _ in 0..1000 {
                    *mutex.lock() += 1;
                }
            });
        }
    });
    assert_eq!(*mutex.lock(), 8000);
}

#[test]
fn a_panic_while_the_lock_is_held_releases_it() {
    let mutex = Mutex::new(0);
    let result = std::panic::catch_unwind(|| {
        let mut held = mutex.lock();
        *held = 3;
        panic!("poisoned? no such thing");
    });
    assert!(result.is_err());
    assert_eq!(*mutex.try_lock().unwrap(), 3);
}

#[test]
fn debug_shows_the_value_or_that_it_is_locked() {
    let mutex = Mutex::new(41);
    assert_eq!(format!("{mutex:?}"), "Mutex { value: 41 }");
    let held = mutex.lock();
    assert_eq!(format!("{mutex:?}"), "Mutex { value: \"<locked>\" }");
    drop(held);
}

#[test]
fn lazy_computes_on_first_access_and_keeps_the_value() {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let lazy: Lazy<usize> = Lazy::new(|| {
        CALLS.fetch_add(1, Ordering::Relaxed);
        99
    });
    assert_eq!(CALLS.load(Ordering::Relaxed), 0);
    assert_eq!(*lazy, 99);
    assert_eq!(*lazy, 99);
    assert_eq!(CALLS.load(Ordering::Relaxed), 1);
}

#[test]
fn lazy_hands_every_caller_the_same_value() {
    static LAZY: Lazy<Vec<u32>> = Lazy::new(|| vec![1, 2, 3]);
    let addresses: Vec<usize> = thread::scope(|scope| {
        let first = scope.spawn(|| Lazy::force(&LAZY).as_ptr() as usize);
        let second = scope.spawn(|| LAZY.as_ptr() as usize);
        vec![first.join().unwrap(), second.join().unwrap()]
    });
    assert_eq!(addresses[0], addresses[1]);
    assert_eq!(*LAZY, vec![1, 2, 3]);
}
