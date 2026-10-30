// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `ssh-userauth` service: `none`, `password` and `publickey`
//! requests answered, failures counted against `MaxAuthTries`, the
//! banner sent before the first answer.

use super::{Account, Accounts, PasswordCheck, Peer, access, profile_name};
use crate::config::{Config, pattern};
use crate::io::{FileSystem, files};
use crate::keys::PublicKey;
use crate::keys::authorized::{self, KeyOptions};
use crate::wire::{Reader, WireResult, Writer, msg};

/// How the user got in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// A password.
    Password,
    /// A public key.
    PublicKey,
}

/// A signed-on user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authed {
    /// The profile.
    pub account: Account,
    /// The method.
    pub method: Method,
    /// The key's `authorized_keys` options, for a key.
    pub options: KeyOptions,
}

/// What to do after a request.
#[derive(Debug)]
pub enum Step {
    /// Send these messages and wait for the next request.
    Send(Vec<Vec<u8>>),
    /// Send these, then the user is in.
    Done(Vec<Vec<u8>>, Box<Authed>),
    /// Disconnect with this text.
    Disconnect(u32, String),
}

/// The state of `ssh-userauth` on one connection.
pub struct UserAuth<'a> {
    cfg: &'a Config,
    accounts: &'a dyn Accounts,
    fs: &'a dyn FileSystem,
    peer: Peer,
    session_id: Vec<u8>,
    failures: u32,
    banner_sent: bool,
    first_user: Option<(String, String)>,
}

impl<'a> UserAuth<'a> {
    /// A new state for a connection.
    #[must_use]
    pub fn new(
        cfg: &'a Config,
        accounts: &'a dyn Accounts,
        fs: &'a dyn FileSystem,
        peer: Peer,
        session_id: Vec<u8>,
    ) -> Self {
        Self {
            cfg,
            accounts,
            fs,
            peer,
            session_id,
            failures: 0,
            banner_sent: false,
            first_user: None,
        }
    }

    /// Failures so far.
    #[must_use]
    pub fn failures(&self) -> u32 {
        self.failures
    }

    fn methods(&self) -> Vec<&'static str> {
        let mut m = Vec::new();
        if self.cfg.pubkey_authentication {
            m.push("publickey");
        }
        if self.cfg.password_authentication {
            m.push("password");
        }
        m
    }

    fn failure(&self) -> Vec<u8> {
        let mut w = Writer::msg(msg::USERAUTH_FAILURE);
        w.names(&self.methods()).bool(false);
        w.finish()
    }

    fn banner(&mut self, out: &mut Vec<Vec<u8>>) {
        if self.banner_sent {
            return;
        }
        self.banner_sent = true;
        let Some(path) = &self.cfg.banner else { return };
        let ifs = self.fs.system();
        if let Ok(text) = files::read_text(&*ifs, path) {
            let mut w = Writer::msg(msg::USERAUTH_BANNER);
            w.str(&text).str("");
            out.push(w.finish());
        }
    }

    fn failed(&mut self, mut out: Vec<Vec<u8>>, counts: bool) -> Step {
        if counts {
            self.failures += 1;
            if self.failures >= self.cfg.max_auth_tries {
                return Step::Disconnect(
                    msg::reason::PROTOCOL_ERROR,
                    "Too many authentication failures".into(),
                );
            }
        }
        out.push(self.failure());
        Step::Send(out)
    }

    /// Answer one `SSH_MSG_USERAUTH_REQUEST`.
    pub fn request(&mut self, m: &[u8]) -> WireResult<Step> {
        let mut r = Reader::new(m);
        r.u8()?;
        let user = r.str()?.to_string();
        let service = r.str()?.to_string();
        let method = r.str()?.to_string();
        if service != "ssh-connection" {
            return Ok(Step::Disconnect(
                msg::reason::SERVICE_NOT_AVAILABLE,
                format!("service {service} is not available"),
            ));
        }
        match &self.first_user {
            None => self.first_user = Some((user.clone(), service)),
            Some((u, _)) if *u != user => {
                return Ok(Step::Disconnect(
                    msg::reason::PROTOCOL_ERROR,
                    "the user name cannot change during authentication".into(),
                ));
            }
            Some(_) => {}
        }
        let mut out = Vec::new();
        self.banner(&mut out);
        let account = profile_name(&user).and_then(|n| self.accounts.lookup(&n));
        let account = account.filter(|a| access::permitted(self.cfg, a, &self.peer.addr));
        Ok(match method.as_str() {
            "password" if self.cfg.password_authentication => {
                let change = r.bool()?;
                let password = r.str()?.to_string();
                self.password(out, account, change, &password)
            }
            "publickey" if self.cfg.pubkey_authentication => {
                let signed = r.bool()?;
                let alg = r.str()?.to_string();
                let blob = r.bytes()?.to_vec();
                let sig = if signed { Some(r.bytes()?.to_vec()) } else { None };
                self.publickey(out, account, &user, &alg, &blob, sig.as_deref())
            }
            "none" => self.failed(out, false),
            _ => self.failed(out, true),
        })
    }

    fn password(
        &mut self,
        out: Vec<Vec<u8>>,
        account: Option<Account>,
        change: bool,
        password: &str,
    ) -> Step {
        let Some(account) = account else {
            return self.failed(out, true);
        };
        if change || !access::root_password_ok(self.cfg, &account) {
            return self.failed(out, true);
        }
        if password.is_empty() && !self.cfg.permit_empty_passwords {
            return self.failed(out, true);
        }
        match self
            .accounts
            .check_password(&account.name, password, &self.peer)
        {
            PasswordCheck::Accepted => {
                let mut out = out;
                out.push(vec![msg::USERAUTH_SUCCESS]);
                Step::Done(
                    out,
                    Box::new(Authed {
                        account,
                        method: Method::Password,
                        options: KeyOptions::default(),
                    }),
                )
            }
            PasswordCheck::Expired => {
                tracing::info!(user = account.name, "password expired; refused over SSH");
                self.failed(out, true)
            }
            PasswordCheck::Refused { msgid, .. } => {
                tracing::info!(user = account.name, msgid, "password refused");
                self.failed(out, true)
            }
        }
    }

    fn find_key(&self, account: &Account, key: &PublicKey) -> Option<KeyOptions> {
        let ifs = self.fs.system();
        for path in self.cfg.authorized_keys_for(&account.name, &account.home) {
            let Ok(text) = files::read_text(&*ifs, &path) else {
                continue;
            };
            for line in authorized::parse(&text) {
                if line.key != *key {
                    continue;
                }
                if let Some(from) = &line.options.from
                    && pattern::list(&self.peer.addr, from) != Some(true)
                {
                    continue;
                }
                return Some(line.options);
            }
        }
        None
    }

    fn publickey(
        &mut self,
        mut out: Vec<Vec<u8>>,
        account: Option<Account>,
        user: &str,
        alg: &str,
        blob: &[u8],
        sig: Option<&[u8]>,
    ) -> Step {
        let Some(account) = account else {
            return self.failed(out, true);
        };
        let Some(key) = PublicKey::from_blob(blob).filter(|k| k.signs_with(alg)) else {
            return self.failed(out, true);
        };
        let Some(options) = self.find_key(&account, &key) else {
            return self.failed(out, true);
        };
        if !access::root_key_ok(self.cfg, &account, options.command.is_some()) {
            return self.failed(out, true);
        }
        let Some(sig) = sig else {
            let mut w = Writer::msg(msg::USERAUTH_PK_OK);
            w.str(alg).bytes(blob);
            out.push(w.finish());
            return Step::Send(out);
        };
        let mut w = Writer::new();
        w.bytes(&self.session_id)
            .u8(msg::USERAUTH_REQUEST)
            .str(user)
            .str("ssh-connection")
            .str("publickey")
            .bool(true)
            .str(alg)
            .bytes(blob);
        if !key.verify(sig, &w.finish()) {
            return self.failed(out, true);
        }
        out.push(vec![msg::USERAUTH_SUCCESS]);
        Step::Done(
            out,
            Box::new(Authed {
                account,
                method: Method::PublicKey,
                options,
            }),
        )
    }
}
