// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NEW-ENVIRON (RFC 1572) as TN5250E uses it (RFC 2877, RFC 4777): the
//! `SEND` the server opens with, carrying its random seed, and the `IS`
//! the client answers with its device, keyboard, code page and the
//! auto-signon values.

/// Subnegotiation command: `IS`.
pub const IS: u8 = 0;
/// Subnegotiation command: `SEND`.
pub const SEND: u8 = 1;
/// Subnegotiation command: `INFO`.
pub const INFO: u8 = 2;
/// A well-known variable.
pub const VAR: u8 = 0;
/// A value.
pub const VALUE: u8 = 1;
/// The escape for the four special bytes inside names and values.
pub const ESC: u8 = 2;
/// A user variable.
pub const USERVAR: u8 = 3;

/// One variable of a `SEND` or `IS`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Var {
    /// `VAR` (well known) or `USERVAR`.
    pub kind: u8,
    /// The name.
    pub name: Vec<u8>,
    /// The value; `None` when the variable was named without one
    /// (a `SEND` asking for it, or a defined but empty variable).
    pub value: Option<Vec<u8>>,
}

impl Var {
    /// A `USERVAR` with a text value.
    pub fn user(name: &str, value: &str) -> Self {
        Self {
            kind: USERVAR,
            name: name.as_bytes().to_vec(),
            value: Some(value.as_bytes().to_vec()),
        }
    }

    /// A `USERVAR` with a binary value.
    pub fn user_bytes(name: &str, value: &[u8]) -> Self {
        Self {
            kind: USERVAR,
            name: name.as_bytes().to_vec(),
            value: Some(value.to_vec()),
        }
    }

    /// A well-known `VAR` with a text value.
    pub fn known(name: &str, value: &str) -> Self {
        Self {
            kind: VAR,
            name: name.as_bytes().to_vec(),
            value: Some(value.as_bytes().to_vec()),
        }
    }

    /// The name as text.
    pub fn name_str(&self) -> String {
        String::from_utf8_lossy(&self.name).into_owned()
    }

    /// The value as text, empty when there is none.
    pub fn value_str(&self) -> String {
        self.value
            .as_deref()
            .map(|v| String::from_utf8_lossy(v).into_owned())
            .unwrap_or_default()
    }
}

fn push_escaped(out: &mut Vec<u8>, bytes: &[u8]) {
    for &b in bytes {
        if b <= USERVAR {
            out.push(ESC);
        }
        out.push(b);
    }
}

/// The subnegotiation body (after the option byte) of `command` with
/// these variables: `SEND` lists names (and, for `IBMRSEED`, the seed
/// as the value), `IS` lists names and values.
pub fn encode(command: u8, vars: &[Var]) -> Vec<u8> {
    let mut out = vec![command];
    for v in vars {
        out.push(v.kind);
        push_escaped(&mut out, &v.name);
        if let Some(value) = &v.value {
            out.push(VALUE);
            push_escaped(&mut out, value);
        }
    }
    out
}

/// Parse a subnegotiation body (after the option byte): the command and
/// its variables. A bare `SEND` (no names) asks for everything.
pub fn decode(body: &[u8]) -> Option<(u8, Vec<Var>)> {
    let (&command, mut rest) = body.split_first()?;
    let mut vars = Vec::new();
    let mut cur: Option<Var> = None;
    let mut in_value = false;
    while let Some((&b, tail)) = rest.split_first() {
        rest = tail;
        match b {
            VAR | USERVAR => {
                if let Some(v) = cur.take() {
                    vars.push(v);
                }
                cur = Some(Var {
                    kind: b,
                    name: Vec::new(),
                    value: None,
                });
                in_value = false;
            }
            VALUE => {
                if let Some(v) = cur.as_mut() {
                    v.value = Some(Vec::new());
                    in_value = true;
                }
            }
            ESC => {
                if let Some((&e, tail)) = rest.split_first() {
                    rest = tail;
                    push_byte(cur.as_mut(), in_value, e);
                }
            }
            other => push_byte(cur.as_mut(), in_value, other),
        }
    }
    if let Some(v) = cur {
        vars.push(v);
    }
    Some((command, vars))
}

fn push_byte(cur: Option<&mut Var>, in_value: bool, b: u8) {
    if let Some(v) = cur {
        if in_value {
            if let Some(val) = v.value.as_mut() {
                val.push(b);
            }
        } else {
            v.name.push(b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let vars = vec![
            Var::known("USER", "QSECOFR"),
            Var::user_bytes("IBMRSEED", &[0, 1, 2, 3, 0xFF, 5, 6, 7]),
            Var::user("DEVNAME", "QPADEV0001"),
        ];
        let body = encode(IS, &vars);
        let (cmd, back) = decode(&body).unwrap();
        assert_eq!(cmd, IS);
        assert_eq!(back, vars);
    }

    #[test]
    fn send_without_values() {
        let body = encode(
            SEND,
            &[
                Var {
                    kind: USERVAR,
                    name: b"IBMRSEED".to_vec(),
                    value: Some(vec![9; 8]),
                },
                Var {
                    kind: VAR,
                    name: Vec::new(),
                    value: None,
                },
                Var {
                    kind: USERVAR,
                    name: Vec::new(),
                    value: None,
                },
            ],
        );
        let (cmd, vars) = decode(&body).unwrap();
        assert_eq!(cmd, SEND);
        assert_eq!(vars.len(), 3);
        assert_eq!(vars[0].value.as_deref(), Some(&[9u8; 8][..]));
        assert!(vars[1].name.is_empty() && vars[1].value.is_none());
    }
}
