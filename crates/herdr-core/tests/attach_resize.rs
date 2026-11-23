#![cfg(unix)]
//! `hc attach` repaints the whole screen after a size change. Its outer
//! terminal may keep the top rows when it shrinks (mosh-server does), so
//! output after the resize alone would leave rows of the old screen behind.

mod common;

use common::framed::Viewer;
use common::*;
use herdr_core::wire;
use serde_json::json;

#[test]
fn a_shrunk_attach_gets_the_whole_screen_again() {
    let s = Session::new();
    let inner =
        run(&s, &["sh", "-c", "i=1; while [ $i -le 100 ]; do printf 'hist-%03d\\n' $i; i=$((i+1)); done; sleep 100"]);
    wait_until("history", || text(&s, &inner).contains("hist-100"));
    let bin = s.bin.display().to_string();
    // `hc attach` in a terminal of this core: its pty is the attach's tty.
    let outer = run(&s, &[&bin, "attach", &inner]);
    let mut phone = Viewer::attach(&s, &outer, 53, 47, json!({}));
    let mut watcher = Viewer::attach(&s, &outer, 53, 47, json!({"observe": true}));
    wait_until("47 rows", || get(&s, &inner)["rows"] == 47);
    phone.shows("hist-055");

    // The keyboard opens: 47 -> 30 rows, SIGWINCH for the attach.
    watcher.drain(std::time::Duration::from_millis(300));
    let before = watcher.raw.len();
    phone.conn.send(phone.channel, wire::RESIZE, &[53, 0, 30, 0]);
    wait_until("30 rows", || get(&s, &inner)["rows"] == 30);
    // The screen is now hist-072..hist-100 above the cursor: every row comes again.
    let rows: Vec<String> = (72..=100).map(|n| format!("hist-{n:03}")).collect();
    watcher.until("a full repaint after the resize", |v| {
        let after = String::from_utf8_lossy(&v.raw[before..]);
        rows.iter().all(|row| after.contains(row.as_str()))
    });
}
