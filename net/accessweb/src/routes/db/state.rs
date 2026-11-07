// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a session's database pages keep between requests: Run SQL's
//! connection, options, script and results, and the copies running.
//! Kept by session; a session that has ended loses its work at the next
//! request, its connection closed (uncommitted changes roll back).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex, OnceLock};

use qsweb::msg::Message;

use crate::data::files::{Encoding, Format, Sheet};
use crate::env::db::{Options, ResultColumn, Val};
use crate::routes::Rq;

/// The script a new Run SQL page starts with.
pub const NEW_SCRIPT: &str = "-- statements separated by ;\n";

/// One statement's result.
#[derive(Clone, Debug, Default)]
pub struct Res {
    /// The statement.
    pub statement: String,
    /// A query's columns.
    pub columns: Vec<ResultColumn>,
    /// The rows fetched.
    pub rows: Vec<Vec<Val>>,
    /// The cursor while rows are left.
    pub cursor: Option<u32>,
    /// Rows changed.
    pub changed: i64,
    /// Its message.
    pub message: Option<Message>,
    /// SQLSTATE.
    pub sqlstate: String,
    /// SQLCODE.
    pub sqlcode: i32,
    /// Milliseconds.
    pub ms: u64,
    /// It failed.
    pub failed: bool,
}

impl Res {
    /// A query's result.
    pub fn query(&self) -> bool {
        !self.columns.is_empty()
    }
}

/// Run SQL's page.
#[derive(Clone, Debug)]
pub struct SqlWork {
    /// The page's connection.
    pub conn: Option<u32>,
    /// Its options as last set.
    pub opts: Options,
    /// The script.
    pub script: String,
    /// Stop at the first error.
    pub stop: bool,
    /// CCSID 65535 data as text.
    pub hex: bool,
    /// The last run's results.
    pub results: Vec<Res>,
    /// Changes under commitment control not yet committed.
    pub pending: bool,
    /// `.sql` files opened or saved: name and text, newest first.
    pub recent: Vec<(String, String)>,
    /// The name the script was opened as.
    pub file: String,
}

impl Default for SqlWork {
    fn default() -> Self {
        Self {
            conn: None,
            opts: Options::default(),
            script: NEW_SCRIPT.into(),
            stop: true,
            hex: false,
            results: Vec::new(),
            pending: false,
            recent: Vec::new(),
            file: String::new(),
        }
    }
}

/// A copy running or done.
#[derive(Debug, Default)]
pub struct Job {
    /// Its number in the session.
    pub id: u32,
    /// What it copies (`assets_br1.csv -> HLINV.ASSETS`).
    pub name: String,
    /// Rows done so far.
    pub rows: AtomicU64,
    /// Rows in all, when known.
    pub total: AtomicU64,
    /// Asked to end.
    pub cancel: AtomicBool,
    /// Its messages once it has ended.
    pub done: Mutex<Option<Vec<Message>>>,
    /// Seconds since 1970 it started.
    pub started: u64,
    /// A download's file once written: name, content type, bytes.
    pub file: Mutex<Option<(String, &'static str, Vec<u8>)>>,
    /// Copy to (upload) or from (download).
    pub upload: bool,
}

/// Copy data to table's form: the file read and how it maps.
#[derive(Clone, Debug, Default)]
pub struct UploadForm {
    /// The file's name.
    pub file: String,
    /// Its format.
    pub fmt: Format,
    /// The first row is column names.
    pub header: bool,
    /// Its code page.
    pub enc: Encoding,
    /// What it holds.
    pub sheet: Sheet,
    /// The target schema.
    pub schema: String,
    /// The target table.
    pub table: String,
    /// `append`, `replace` or `update`.
    pub mode: String,
    /// `stop` or `skip`.
    pub on_error: String,
    /// The target column of each source column (`None`: skipped).
    pub map: Vec<Option<String>>,
}

/// A session's work.
#[derive(Debug, Default)]
pub struct Work {
    /// The user.
    pub user: String,
    /// Run SQL.
    pub sql: SqlWork,
    /// Copies.
    pub jobs: Vec<Arc<Job>>,
    /// Copy data to table's form.
    pub upload: Option<UploadForm>,
}

type Store = Mutex<HashMap<(usize, String), Arc<Mutex<Work>>>>;

fn store() -> &'static Store {
    static S: OnceLock<Store> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn key(rq: &Rq<'_>) -> (usize, String) {
    (std::ptr::from_ref(rq.app) as usize, rq.session.id.clone())
}

/// The session's work.
pub fn work(rq: &Rq<'_>) -> Arc<Mutex<Work>> {
    let mut s = store().lock().expect("work");
    s.entry(key(rq))
        .or_insert_with(|| {
            Arc::new(Mutex::new(Work {
                user: rq.user().to_string(),
                ..Work::default()
            }))
        })
        .clone()
}

/// Drop the work of this product's sessions that have ended, closing
/// their connections.
pub fn prune(rq: &Rq<'_>) {
    let app = std::ptr::from_ref(rq.app) as usize;
    let gone: Vec<Arc<Mutex<Work>>> = {
        let mut s = store().lock().expect("work");
        let dead: Vec<(usize, String)> = s
            .keys()
            .filter(|(a, id)| *a == app && rq.app.sessions.get(id).is_none())
            .cloned()
            .collect();
        dead.iter().filter_map(|k| s.remove(k)).collect()
    };
    for w in gone {
        let w = w.lock().expect("work");
        if let Some(c) = w.sql.conn {
            rq.env().db.close(&w.user, c);
        }
    }
}
