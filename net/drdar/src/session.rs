// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A job's SQL connections, as the Db2 for i application requester
//! keeps them: the local server and the servers `CONNECT` reached, one
//! of them current; distributed (`*DUW`) or remote (`*RUW`) units of
//! work. A statement the job runs comes here first ([`Sessions::route`]):
//! a connection statement is answered here, any other goes to the
//! current server when that is not the local one.

use std::collections::{HashMap, VecDeque};

use sql::embed::{Op, Reply, Request};

use crate::values::coded;
use crate::verbs::{Operand, Target, Verb, classify};

/// One connection to a server, as the sessions use it.
pub trait Server {
    /// Run a query, every row read.
    ///
    /// # Errors
    ///
    /// The SQLCA of the failure.
    fn query(&mut self, text: &str) -> Result<Vec<Vec<sql::Value>>, sql::Sqlca>;
    /// Run a statement at once.
    fn execute(&mut self, text: &str) -> sql::Sqlca;
    /// Commit.
    fn commit(&mut self) -> sql::Sqlca;
    /// Roll back.
    fn rollback(&mut self) -> sql::Sqlca;
}

/// What makes connections: the job's relational database directory and
/// its network.
pub trait Connector {
    /// The local server's name (the `*LOCAL` entry).
    fn local(&self) -> String;
    /// Connect to `rdb`, as `user` with its password when given.
    ///
    /// # Errors
    ///
    /// The SQLCA of the failure: `-950` for a name the directory lacks,
    /// `-30080` and `-30082` for the network and the server's refusal.
    fn connect(
        &mut self,
        rdb: &str,
        user: Option<(&str, &str)>,
    ) -> Result<Box<dyn Server>, sql::Sqlca>;
}

/// `RDBCNNMTH`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Method {
    /// Distributed unit of work: several connections.
    #[default]
    Duw,
    /// Remote unit of work: one at a time.
    Ruw,
}

/// The current connection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Current {
    /// The local server.
    #[default]
    Local,
    /// A server `CONNECT` reached.
    Remote(String),
    /// None: the current one was released or disconnected.
    None,
}

struct Conn {
    rdb: String,
    server: Box<dyn Server>,
    release: bool,
    changed: bool,
    cursors: HashMap<(String, String), VecDeque<Vec<sql::Value>>>,
    prepared: HashMap<(String, String), String>,
}

/// What became of a statement.
pub enum Routed {
    /// Answered here.
    Done(Reply),
    /// The local server runs it.
    Local,
}

/// A job's connections.
#[derive(Default)]
pub struct Sessions {
    conns: Vec<Conn>,
    /// The current connection.
    pub current: Current,
    /// `RDBCNNMTH`.
    pub method: Method,
    local_release: bool,
}

fn reply(sqlca: sql::Sqlca) -> Routed {
    Routed::Done(Reply {
        sqlca,
        rows: Vec::new(),
        call: None,
    })
}

fn ok() -> Routed {
    reply(sql::Sqlca::default())
}

fn not_connected(rdb: &str) -> Routed {
    reply(coded(
        -843,
        "08003",
        &format!("there is no connection to {rdb}"),
    ))
}

/// The text of an operand.
fn text_of(o: &Operand, inputs: &[sql::Value]) -> String {
    match o {
        Operand::Text(t) => t.clone(),
        Operand::Input(i) => inputs
            .get(*i)
            .map(|v| match v {
                sql::Value::Str(s) => s.clone(),
                v => v.to_string(),
            })
            .unwrap_or_default(),
    }
}

impl Sessions {
    /// Whether any server but the local one is connected.
    #[must_use]
    pub fn remote(&self) -> bool {
        !self.conns.is_empty()
    }

    /// The servers connected, the current one marked.
    #[must_use]
    pub fn servers(&self) -> Vec<(String, bool)> {
        self.conns
            .iter()
            .map(|c| {
                (
                    c.rdb.clone(),
                    self.current == Current::Remote(c.rdb.clone()),
                )
            })
            .collect()
    }

    fn find(&mut self, rdb: &str) -> Option<&mut Conn> {
        self.conns
            .iter_mut()
            .find(|c| c.rdb.eq_ignore_ascii_case(rdb))
    }

    /// Answer `req`, or leave it to the local server.
    pub fn route(&mut self, c: &mut dyn Connector, req: &Request) -> Routed {
        let d = &req.desc;
        if d.op == Op::Exec {
            match classify(&d.text) {
                Verb::Other => {}
                Verb::Syntax(at) => return reply(coded(-104, "42601", &at)),
                Verb::Commit => return self.end_unit(true),
                Verb::Rollback => return self.end_unit(false),
                v => return self.connection(c, &v, &req.inputs),
            }
        }
        let rdb = match &self.current {
            Current::Local => return Routed::Local,
            Current::None => {
                return match d.op {
                    Op::End | Op::Nothing | Op::Deallocate => Routed::Local,
                    _ => not_connected("the current server"),
                };
            }
            Current::Remote(r) => r.clone(),
        };
        let scope = req.scope.clone();
        let Some(conn) = self.find(&rdb) else {
            return not_connected(&rdb);
        };
        conn.run(&scope, req)
    }

    fn connection(&mut self, c: &mut dyn Connector, v: &Verb, inputs: &[sql::Value]) -> Routed {
        let local = c.local();
        match v {
            Verb::ConnectInfo => ok(),
            Verb::ConnectReset => self.connect_local(),
            Verb::Connect { rdb, user, using } => {
                let rdb = text_of(rdb, inputs).trim().to_ascii_uppercase();
                if rdb.eq_ignore_ascii_case(&local) {
                    return self.connect_local();
                }
                if self.method == Method::Duw && self.find(&rdb).is_some() {
                    return reply(coded(-842, "08002", &rdb));
                }
                if self.method == Method::Ruw {
                    if self.conns.iter().any(|x| x.changed) {
                        return reply(coded(-752, "0A001", "the unit of work has changes"));
                    }
                    self.conns.clear();
                }
                let creds = match (user, using) {
                    (Some(u), Some(p)) => Some((
                        text_of(u, inputs).trim().to_ascii_uppercase(),
                        text_of(p, inputs),
                    )),
                    _ => None,
                };
                match c.connect(&rdb, creds.as_ref().map(|(u, p)| (u.as_str(), p.as_str()))) {
                    Ok(server) => {
                        self.conns.push(Conn {
                            rdb: rdb.clone(),
                            server,
                            release: false,
                            changed: false,
                            cursors: HashMap::new(),
                            prepared: HashMap::new(),
                        });
                        self.current = Current::Remote(rdb);
                        ok()
                    }
                    Err(e) => reply(e),
                }
            }
            Verb::SetConnection(o) => {
                let rdb = text_of(o, inputs).trim().to_ascii_uppercase();
                if rdb.eq_ignore_ascii_case(&local) {
                    self.current = Current::Local;
                    return ok();
                }
                if self.find(&rdb).is_none() {
                    return not_connected(&rdb);
                }
                self.current = Current::Remote(rdb);
                ok()
            }
            Verb::Release(t) => self.each(&local, t, inputs, |s, rdb| {
                match rdb {
                    None => s.local_release = true,
                    Some(r) => {
                        if let Some(c) = s.find(r) {
                            c.release = true;
                        }
                    }
                }
                Ok(())
            }),
            Verb::Disconnect(t) => self.each(&local, t, inputs, |s, rdb| {
                match rdb {
                    None => {
                        if s.current == Current::Local {
                            s.current = Current::None;
                        }
                    }
                    Some(r) => {
                        if s.find(r).is_some_and(|c| c.changed) {
                            return Err(coded(-752, "0A001", "the unit of work has changes"));
                        }
                        s.conns.retain(|c| !c.rdb.eq_ignore_ascii_case(r));
                        if s.current == Current::Remote(r.to_string()) {
                            s.current = Current::None;
                        }
                    }
                }
                Ok(())
            }),
            Verb::Commit | Verb::Rollback | Verb::Other | Verb::Syntax(_) => Routed::Local,
        }
    }

    /// `f` for each connection `t` names: `None` for the local one.
    fn each(
        &mut self,
        local: &str,
        t: &Target,
        inputs: &[sql::Value],
        mut f: impl FnMut(&mut Self, Option<&str>) -> Result<(), sql::Sqlca>,
    ) -> Routed {
        let names: Vec<Option<String>> = match t {
            Target::All => std::iter::once(None)
                .chain(self.conns.iter().map(|c| Some(c.rdb.clone())))
                .collect(),
            Target::Current => match &self.current {
                Current::Local => vec![None],
                Current::Remote(r) => vec![Some(r.clone())],
                Current::None => return not_connected("the current server"),
            },
            Target::Named(o) => {
                let rdb = text_of(o, inputs).trim().to_ascii_uppercase();
                if rdb.eq_ignore_ascii_case(local) {
                    vec![None]
                } else if self.find(&rdb).is_some() {
                    vec![Some(rdb)]
                } else {
                    return not_connected(&rdb);
                }
            }
        };
        for n in names {
            if let Err(e) = f(self, n.as_deref()) {
                return reply(e);
            }
        }
        ok()
    }

    fn connect_local(&mut self) -> Routed {
        if self.method == Method::Ruw {
            if self.conns.iter().any(|x| x.changed) {
                return reply(coded(-752, "0A001", "the unit of work has changes"));
            }
            self.conns.clear();
        }
        self.local_release = false;
        self.current = Current::Local;
        ok()
    }

    /// `COMMIT` or `ROLLBACK`: every server's unit of work ends, the
    /// released connections go; the local server's ends after.
    fn end_unit(&mut self, commit: bool) -> Routed {
        for c in &mut self.conns {
            let ca = if commit {
                c.server.commit()
            } else {
                c.server.rollback()
            };
            if ca.is_error() {
                return reply(ca);
            }
            c.changed = false;
            c.cursors.clear();
        }
        let current = self.current.clone();
        let released: Vec<String> = self
            .conns
            .iter()
            .filter(|c| c.release)
            .map(|c| c.rdb.clone())
            .collect();
        self.conns.retain(|c| !c.release);
        if let Current::Remote(r) = &current
            && released.contains(r)
        {
            self.current = Current::None;
        }
        if std::mem::take(&mut self.local_release) && current == Current::Local {
            self.current = Current::None;
        }
        Routed::Local
    }
}

/// The statement text of an `EXECUTE IMMEDIATE` or `PREPARE`.
fn dynamic_text(req: &Request) -> String {
    match req.inputs.first() {
        Some(sql::Value::Str(s)) => s.clone(),
        _ => req.desc.text.clone(),
    }
}

fn refused(what: &str) -> Routed {
    reply(coded(-84, "42612", what))
}

impl Conn {
    fn run(&mut self, scope: &str, req: &Request) -> Routed {
        let d = &req.desc;
        let key = (scope.to_string(), d.name.clone());
        // BLOCKED(rel): the client sends no input values (SQLDTA) yet.
        let no_inputs = |n: usize| req.inputs.len() <= n;
        match d.op {
            Op::Query => {
                if !no_inputs(0) {
                    return refused("host variables in a statement for a remote server");
                }
                match self.server.query(&d.text) {
                    Ok(rows) => Routed::Done(match rows.len() {
                        0 => Reply {
                            sqlca: sql::Sqlca::no_data(),
                            rows: Vec::new(),
                            call: None,
                        },
                        1 => Reply {
                            sqlca: sql::Sqlca::rows(1),
                            rows,
                            call: None,
                        },
                        _ => Reply {
                            sqlca: coded(-811, "21000", ""),
                            rows: rows.into_iter().take(1).collect(),
                            call: None,
                        },
                    }),
                    Err(e) => reply(e),
                }
            }
            Op::Exec | Op::ExecImm | Op::Execute => {
                let (text, skip) = match d.op {
                    Op::Exec => (d.text.clone(), 0),
                    Op::ExecImm => (dynamic_text(req), 1),
                    _ => match self.prepared.get(&key) {
                        Some(t) => (t.clone(), 0),
                        None => return reply(coded(-518, "07003", &d.name)),
                    },
                };
                if !no_inputs(skip) {
                    return refused("host variables in a statement for a remote server");
                }
                let ca = self.server.execute(&text);
                if !ca.is_error() && !text.trim_start().to_ascii_uppercase().starts_with("SELECT") {
                    self.changed = true;
                }
                reply(ca)
            }
            Op::Prepare => {
                self.prepared.insert(key, dynamic_text(req));
                ok()
            }
            Op::Deallocate => {
                self.prepared.remove(&key);
                ok()
            }
            Op::Open => {
                if !no_inputs(0) {
                    return refused("host variables in a statement for a remote server");
                }
                let text = if d.stmt.is_empty() {
                    d.text.clone()
                } else {
                    match self.prepared.get(&(scope.to_string(), d.stmt.clone())) {
                        Some(t) => t.clone(),
                        None => return reply(coded(-514, "26501", &d.stmt)),
                    }
                };
                if self.cursors.contains_key(&key) {
                    return reply(coded(-502, "24502", &d.name));
                }
                match self.server.query(&text) {
                    Ok(rows) => {
                        self.cursors.insert(key, rows.into());
                        ok()
                    }
                    Err(e) => reply(e),
                }
            }
            Op::Fetch => {
                let orient = d.orient.trim().to_ascii_uppercase();
                if !(orient.is_empty() || orient == "NEXT") {
                    return refused("a scrolling FETCH from a remote server");
                }
                let Some(rows) = self.cursors.get_mut(&key) else {
                    return reply(coded(-501, "24501", &d.name));
                };
                let n = if d.rows {
                    req.inputs
                        .last()
                        .and_then(|v| v.as_i64().ok())
                        .and_then(|n| usize::try_from(n).ok())
                        .unwrap_or(1)
                } else {
                    1
                };
                let got: Vec<_> = (0..n).map_while(|_| rows.pop_front()).collect();
                if got.is_empty() {
                    return reply(sql::Sqlca::no_data());
                }
                let mut ca = sql::Sqlca::rows(got.len() as u64);
                ca.sqlerrd[2] = i32::try_from(got.len()).unwrap_or(i32::MAX);
                if d.rows && rows.is_empty() {
                    // The last block: SQLERRD(5) says no more follow.
                    ca.sqlerrd[4] = 100;
                }
                Routed::Done(Reply {
                    sqlca: ca,
                    rows: got,
                    call: None,
                })
            }
            Op::Close => {
                if self.cursors.remove(&key).is_none() {
                    return reply(coded(-501, "24501", &d.name));
                }
                ok()
            }
            Op::End => {
                self.cursors.retain(|(s, _), _| s != scope);
                self.prepared.retain(|(s, _), _| s != scope);
                Routed::Local
            }
            Op::Nothing | Op::Called | Op::Routed => Routed::Local,
            Op::Describe => refused("DESCRIBE of a statement for a remote server"),
            Op::Diag => refused("GET DIAGNOSTICS after a statement a remote server ran"),
            Op::Package => refused("SQL package requests for a remote server"),
        }
    }
}
