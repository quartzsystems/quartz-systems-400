// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading lines typed on the serial port.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::mpsc::Sender;
use std::thread;

use crate::ipl::Event;
use crate::platform::console::{SERIAL, open_read};

/// Forward each line from the serial port as `Event::Line`. Returns false
/// when the port cannot be opened for reading.
pub fn spawn_reader(tx: Sender<Event>) -> bool {
    let Some(file) = open_read(SERIAL) else {
        return false;
    };
    thread::spawn(move || pump(file, &tx));
    true
}

fn pump(file: File, tx: &Sender<Event>) {
    for chunk in BufReader::new(file).split(b'\n').map_while(Result::ok) {
        let line = String::from_utf8_lossy(&chunk).trim().to_string();
        if tx.send(Event::Line(line)).is_err() {
            break;
        }
    }
}
