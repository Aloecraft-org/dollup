//! One `dollup` invocation helper for every integration test, with a HOME
//! of its own per test thread.
//!
//! A root's store is the cache under `~/.dollup/`, shared by every root on
//! the box, and every verb that opens a root records it in `~/.dollup/
//! roots.json`. Tests running in parallel against one real HOME would
//! share both: a `gc` in one test could sweep a blob another had just put
//! and not yet locked, and every test root would land on the developer's
//! own list of roots. So each test thread gets a fresh temporary HOME,
//! created on first use and kept for the thread's life; a test that needs
//! to look inside HOME sets its own with `.env("HOME", …)`, which wins.

use std::cell::RefCell;
use std::process::Command;

thread_local! {
    static HOME: RefCell<Option<tempfile::TempDir>> = const { RefCell::new(None) };
}

#[allow(dead_code)]
pub fn dollup() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dollup"));
    HOME.with(|home| {
        let mut home = home.borrow_mut();
        let dir = home.get_or_insert_with(|| tempfile::tempdir().expect("a temporary HOME"));
        cmd.env("HOME", dir.path());
    });
    cmd
}
