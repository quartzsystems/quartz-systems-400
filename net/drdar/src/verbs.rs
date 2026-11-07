// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statements the requester answers itself: `CONNECT`, `SET
//! CONNECTION`, `RELEASE`, `DISCONNECT`, and the `COMMIT` and
//! `ROLLBACK` that end every connection's unit of work.

use sql::lex::{Tok, tokenize};

/// An operand: a name or constant written in the statement, or the
/// input value at a position (`?` or a host variable).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operand {
    /// Written in the statement.
    Text(String),
    /// The input at this position.
    Input(usize),
}

/// Which connections `RELEASE` and `DISCONNECT` name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// One, by name.
    Named(Operand),
    /// The current one.
    Current,
    /// Every one (`ALL [SQL]`).
    All,
}

/// What a statement asks of the requester.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verb {
    /// `CONNECT` alone: the current connection's state.
    ConnectInfo,
    /// `CONNECT RESET`: back to the local server.
    ConnectReset,
    /// `CONNECT TO rdb [USER u USING p]`.
    Connect {
        /// The server.
        rdb: Operand,
        /// The user.
        user: Option<Operand>,
        /// The password.
        using: Option<Operand>,
    },
    /// `SET CONNECTION rdb`.
    SetConnection(Operand),
    /// `RELEASE`.
    Release(Target),
    /// `DISCONNECT`.
    Disconnect(Target),
    /// `COMMIT [WORK] [HOLD]`.
    Commit,
    /// `ROLLBACK [WORK] [HOLD]` (not `TO SAVEPOINT`).
    Rollback,
    /// Anything else.
    Other,
    /// One of these statements written wrongly: the token where it went
    /// wrong.
    Syntax(String),
}

struct Words {
    toks: Vec<Tok>,
    at: usize,
    inputs: usize,
}

impl Words {
    fn word(&self, i: usize) -> Option<&str> {
        match self.toks.get(self.at + i) {
            Some(Tok::Word(w)) => Some(w.as_str()),
            _ => None,
        }
    }

    fn eat(&mut self, w: &str) -> bool {
        if self.word(0) == Some(w) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn operand(&mut self) -> Option<Operand> {
        let t = self.toks.get(self.at)?.clone();
        self.at += 1;
        match t {
            Tok::Word(w) => Some(Operand::Text(w)),
            Tok::Delimited(d) | Tok::Str(d) => Some(Operand::Text(d)),
            Tok::Param | Tok::Host { .. } => {
                let n = self.inputs;
                self.inputs += 1;
                Some(Operand::Input(n))
            }
            _ => None,
        }
    }

    fn end(&self) -> bool {
        matches!(self.toks.get(self.at), None | Some(Tok::End))
            || matches!(self.toks.get(self.at), Some(Tok::Sym(";")))
    }

    fn here(&self) -> String {
        self.toks
            .get(self.at)
            .map_or_else(String::new, Tok::describe)
    }

    fn target(&mut self) -> Option<Target> {
        if self.eat("CURRENT") {
            return Some(Target::Current);
        }
        if self.word(0) == Some("ALL") {
            self.at += 1;
            self.eat("SQL");
            return Some(Target::All);
        }
        self.operand().map(Target::Named)
    }
}

/// What `text` asks of the requester.
#[must_use]
pub fn classify(text: &str) -> Verb {
    let Ok(toks) = tokenize(text) else {
        return Verb::Other;
    };
    let mut w = Words {
        toks: toks.into_iter().map(|t| t.tok).collect(),
        at: 0,
        inputs: 0,
    };
    let first = w.word(0).map(str::to_string);
    let verb = match first.as_deref() {
        Some("CONNECT") => {
            w.at += 1;
            if w.end() {
                return Verb::ConnectInfo;
            }
            if w.eat("RESET") {
                Verb::ConnectReset
            } else if w.eat("TO") {
                let Some(rdb) = w.operand() else {
                    return Verb::Syntax(w.here());
                };
                let (mut user, mut using) = (None, None);
                if w.eat("USER") {
                    user = w.operand();
                    if user.is_none() || !w.eat("USING") {
                        return Verb::Syntax(w.here());
                    }
                    using = w.operand();
                    if using.is_none() {
                        return Verb::Syntax(w.here());
                    }
                }
                Verb::Connect { rdb, user, using }
            } else {
                return Verb::Syntax(w.here());
            }
        }
        Some("SET") if w.word(1) == Some("CONNECTION") => {
            w.at += 2;
            match w.operand() {
                Some(o) => Verb::SetConnection(o),
                None => return Verb::Syntax(w.here()),
            }
        }
        Some("RELEASE") if !matches!(w.word(1), Some("SAVEPOINT" | "TO")) => {
            w.at += 1;
            match w.target() {
                Some(t) => Verb::Release(t),
                None => return Verb::Syntax(w.here()),
            }
        }
        Some("DISCONNECT") => {
            w.at += 1;
            match w.target() {
                Some(t) => Verb::Disconnect(t),
                None => return Verb::Syntax(w.here()),
            }
        }
        Some("COMMIT") => return Verb::Commit,
        Some("ROLLBACK") if !text.to_ascii_uppercase().contains("SAVEPOINT") => {
            return Verb::Rollback;
        }
        _ => return Verb::Other,
    };
    if w.end() {
        verb
    } else {
        Verb::Syntax(w.here())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_statements() {
        assert_eq!(classify("CONNECT"), Verb::ConnectInfo);
        assert_eq!(classify("connect reset"), Verb::ConnectReset);
        assert_eq!(
            classify("CONNECT TO S062B175 USER ? USING ?"),
            Verb::Connect {
                rdb: Operand::Text("S062B175".into()),
                user: Some(Operand::Input(0)),
                using: Some(Operand::Input(1))
            }
        );
        assert_eq!(
            classify("CONNECT TO :rdb"),
            Verb::Connect {
                rdb: Operand::Input(0),
                user: None,
                using: None
            }
        );
        assert_eq!(
            classify("SET CONNECTION REMOTE1"),
            Verb::SetConnection(Operand::Text("REMOTE1".into()))
        );
        assert_eq!(classify("RELEASE ALL SQL"), Verb::Release(Target::All));
        assert_eq!(classify("RELEASE CURRENT"), Verb::Release(Target::Current));
        assert_eq!(
            classify("DISCONNECT X"),
            Verb::Disconnect(Target::Named(Operand::Text("X".into())))
        );
        assert_eq!(classify("RELEASE SAVEPOINT A"), Verb::Other);
        assert_eq!(classify("ROLLBACK TO SAVEPOINT A"), Verb::Other);
        assert_eq!(classify("COMMIT WORK"), Verb::Commit);
        assert_eq!(classify("SELECT 1 FROM SYSIBM.SYSDUMMY1"), Verb::Other);
        assert!(matches!(classify("CONNECT TO X USER Y"), Verb::Syntax(_)));
        assert!(matches!(classify("CONNECT X"), Verb::Syntax(_)));
    }
}
