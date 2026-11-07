// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The close lines `qapd` logs, kept by field: a subscriber for the
//! whole test process.

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Mutex, Once, PoisonError};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

/// One event's fields, with `level` and `target`.
pub type Line = HashMap<String, String>;

static LINES: Mutex<Vec<Line>> = Mutex::new(Vec::new());
static ECHO: Mutex<Option<fn(&Line)>> = Mutex::new(None);

struct Capture;

struct Fields(Line);

impl Visit for Fields {
    fn record_debug(&mut self, f: &Field, v: &dyn Debug) {
        self.0.insert(f.name().to_string(), format!("{v:?}"));
    }
    fn record_str(&mut self, f: &Field, v: &str) {
        self.0.insert(f.name().to_string(), v.to_string());
    }
}

impl Subscriber for Capture {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, e: &Event<'_>) {
        let mut f = Fields(Line::new());
        e.record(&mut f);
        f.0.insert("level".into(), e.metadata().level().to_string());
        f.0.insert("target".into(), e.metadata().target().to_string());
        if let Some(echo) = *ECHO.lock().unwrap_or_else(PoisonError::into_inner) {
            echo(&f.0);
        }
        LINES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(f.0);
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

/// Keep every event from now on; `echo` sees each as it comes.
pub fn capture(echo: Option<fn(&Line)>) {
    static INIT: Once = Once::new();
    *ECHO.lock().unwrap_or_else(PoisonError::into_inner) = echo;
    INIT.call_once(|| {
        tracing::subscriber::set_global_default(Capture).expect("the only subscriber");
    });
}

/// The lines of connection `connection`.
pub fn lines(connection: u32) -> Vec<Line> {
    let id = connection.to_string();
    LINES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|l| l.get("connection") == Some(&id))
        .cloned()
        .collect()
}
