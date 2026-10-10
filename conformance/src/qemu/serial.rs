// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Line-oriented capture of QEMU's stdout (serial) and stderr.

use std::io::{BufRead, BufReader, Read};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

/// Forward lines from both pipes into one channel. The channel closes
/// when both pipes hit EOF, which is how QEMU's exit is noticed.
pub fn stream<O, E>(stdout: Option<O>, stderr: Option<E>) -> Receiver<String>
where
    O: Read + Send + 'static,
    E: Read + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    if let Some(out) = stdout {
        pump(out, tx.clone(), "");
    }
    if let Some(err) = stderr {
        pump(err, tx, "[qemu] ");
    }
    rx
}

fn pump<R: Read + Send + 'static>(reader: R, tx: Sender<String>, prefix: &'static str) {
    thread::spawn(move || {
        for chunk in BufReader::new(reader).split(b'\n').map_while(Result::ok) {
            let text = String::from_utf8_lossy(&chunk);
            let line = format!("{prefix}{}", text.trim_end_matches('\r'));
            if tx.send(line).is_err() {
                break;
            }
        }
    });
}
