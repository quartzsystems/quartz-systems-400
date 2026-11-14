// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Extended operations (docs/DIRSRV.md §7): `StartTLS` (the job's
//! handshake: [`Outcome::start_tls`](super::Outcome::start_tls)), whoami
//! (RFC 4532) and password modify (RFC 3062), QS/400 additions. Every
//! other request, IBM's `1.3.18.0.2.12.*` included, is protocolError (2)
//! as IBM's server answers one it does not know.

use ldap::ResultCode;
use ldap::control::{ControlValue, PasswordPolicyResponse};
use ldap::ext::{self, PasswordModifyRequest, PasswordModifyResponse};
use ldap::msg::{Attribute, LdapMessage, LdapResult, ModOp, Modification, ProtocolOp};
use store::txn::Txn;

use super::{Directory, OpError, OpResult, Outcome, Session};

/// The extended operations QS/400 advertises in `supportedExtension`.
pub(crate) const SUPPORTED: [&str; 3] = [ext::START_TLS, ext::WHO_AM_I, ext::PASSWORD_MODIFY];

fn ext_reply(result: LdapResult, name: Option<&str>, value: Option<Vec<u8>>) -> ProtocolOp {
    ProtocolOp::ExtendedResponse {
        result,
        name: name.map(String::from),
        value,
    }
}

impl Directory {
    pub(crate) fn extended(
        &mut self,
        txn: &mut Txn<'_>,
        s: &Session,
        msg: &LdapMessage,
    ) -> Outcome {
        let ProtocolOp::ExtendedRequest { name, value } = &msg.op else {
            return Outcome::default();
        };
        // Whoami and password modify run as the proxied identity, when
        // the control names one (RFC 4370).
        let acting = super::controls::parse(
            msg,
            super::controls::Op::Extended,
            &self.settings,
            s.admin,
        )
        .and_then(|ctl| self.acting(txn, s, ctl.proxy.as_deref()));
        let acting = match acting {
            Ok(a) => a,
            Err(e) => return Outcome::one(ext_reply(e.result(), None, None), Vec::new()),
        };
        let s = &acting;
        let want_policy = msg
            .controls
            .iter()
            .any(|c| c.oid == ldap::control::PASSWORD_POLICY);
        match name.as_str() {
            ext::START_TLS => {
                if value.is_some() {
                    return Outcome::one(
                        ext_reply(
                            Self::result(ResultCode::ProtocolError, "StartTLS takes no value"),
                            None,
                            None,
                        ),
                        Vec::new(),
                    );
                }
                let mut o = Outcome::one(
                    ext_reply(LdapResult::success(), Some(ext::START_TLS), None),
                    Vec::new(),
                );
                o.start_tls = true;
                o
            }
            ext::WHO_AM_I => {
                let id =
                    s.dn.as_ref()
                        .map_or_else(String::new, |d| format!("dn:{d}"));
                Outcome::one(
                    ext::who_am_i_response(LdapResult::success(), &id),
                    Vec::new(),
                )
            }
            ext::PASSWORD_MODIFY => match self.password_modify(txn, s, value.as_deref()) {
                Ok((generated, changes)) => {
                    let mut o = Outcome::one(
                        ext_reply(
                            LdapResult::success(),
                            None,
                            PasswordModifyResponse { generated }.encode_value(),
                        ),
                        Vec::new(),
                    );
                    o.changes = changes;
                    o
                }
                Err(e) => {
                    let mut ctls = Vec::new();
                    if want_policy && e.policy.is_some() {
                        ctls.push(
                            PasswordPolicyResponse {
                                warning: None,
                                error: e.policy,
                            }
                            .to_control(false),
                        );
                    }
                    Outcome::one(ext_reply(e.result(), None, None), ctls)
                }
            },
            _ => Outcome::one(
                ext_reply(
                    Self::result(
                        ResultCode::ProtocolError,
                        "the extended operation is not supported",
                    ),
                    None,
                    None,
                ),
                Vec::new(),
            ),
        }
    }

    /// RFC 3062: the user's (or the bound identity's) `userPassword`
    /// replaced, the old one checked when given, a new one generated
    /// when none is.
    fn password_modify(
        &mut self,
        txn: &mut Txn<'_>,
        s: &Session,
        value: Option<&[u8]>,
    ) -> OpResult<(Option<Vec<u8>>, Vec<super::Change>)> {
        let req = PasswordModifyRequest::decode_value(value).map_err(|_| {
            OpError::new(ResultCode::ProtocolError, "the request value is malformed")
        })?;
        let user = match req.user.as_deref() {
            Some(u) if !u.is_empty() => u.strip_prefix("dn:").unwrap_or(u).to_string(),
            _ => {
                s.dn.clone()
                    .ok_or_else(|| OpError::unwilling("an anonymous session has no password"))?
            }
        };
        let d = Self::parse_dn(&user)?;
        let f = self.find_visible(txn, &self.sys.clone(), &d, false)?;
        if let Some(old) = &req.old {
            let ok = f
                .entry
                .get("userPassword")
                .is_some_and(|a| a.vals.iter().any(|v| crate::pwd::verify(v, old)));
            if !ok {
                return Err(OpError::new(
                    ResultCode::InvalidCredentials,
                    "the old password is not right",
                ));
            }
        }
        let (new, generated) = if let Some(n) = req.new {
            (n, None)
        } else {
            let mut b = [0u8; 12];
            self.env.random(&mut b);
            let g = crate::pwd::crypt::salt_chars(&b).into_bytes();
            (g.clone(), Some(g))
        };
        let mut changes = Vec::new();
        if let Some(old) = &req.old {
            changes.push(Modification {
                op: ModOp::Delete,
                attr: Attribute {
                    ty: "userPassword".into(),
                    vals: vec![old.clone()],
                },
            });
            changes.push(Modification {
                op: ModOp::Add,
                attr: Attribute {
                    ty: "userPassword".into(),
                    vals: vec![new],
                },
            });
        } else {
            changes.push(Modification {
                op: ModOp::Replace,
                attr: Attribute {
                    ty: "userPassword".into(),
                    vals: vec![new],
                },
            });
        }
        let msg = LdapMessage::new(
            0,
            ProtocolOp::ModifyRequest {
                dn: f.entry.dn.clone(),
                changes,
            },
        );
        let ctl = super::controls::Ctl::default();
        let done = self.modify_for_ext(txn, s, &ctl, &msg)?;
        Ok((generated, done))
    }
}
