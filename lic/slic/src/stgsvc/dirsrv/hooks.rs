// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The directory's hooks over `sec` (docs/DIRSRV.md §9, §12).
//!
//! [`SecIdentity`]: binds of projected DNs, checked by sign-on's own
//! `sec::signon::sign_on` (no device), so `*DISABLED`, `QMAXSIGN`,
//! `QMAXSGNACN` and the `PW` audit entry are sign-on's; an expired
//! password is 49 with `passwordExpired`; `QIBM_DIRSRV_ADMIN` allowed
//! makes the session an administrator's; the profile's groups are the
//! session's, as projected DNs. SASL EXTERNAL with a subject DN that is
//! a projected DN takes the profile when it is enabled and has a
//! password; any other subject needs an entry (the operations look).
//!
//! [`SecAudit`]: each operation as a `DI` entry (`sec::audit::di`) when
//! `ibm-audit` and the operation's switch select it.

use std::cell::RefCell;
use std::rc::Rc;

use dirsrv::backend::projected::Names;
use dirsrv::config::settings::AuditSwitches;
use dirsrv::ops::{Audit, AuditEvent, AuditOp, Granted, Identity, Refused};
use dirsrv::schema::Schema;
use ldap::ResultCode;
use ldap::control::{PolicyError, PolicyWarning};
use ldap::dn::Dn;
use sec::audit::di::{DiFields, Op};
use sec::auth::principal::Principal;
use sec::signon::{Attempt, Next, Values};
use store::obj::ObjectId;
use store::txn::Txn;

/// The function usage that makes a profile's bind an administrator's.
pub const ADMIN_FUNCTION: &str = "QIBM_DIRSRV_ADMIN";

/// The password modify extended operation (RFC 3062).
const PASSWORD_MODIFY: &str = "1.3.6.1.4.1.4203.1.11.1";

/// What a bind needs from the system, refreshed before each request.
#[derive(Debug, Clone)]
pub struct BindCtx {
    /// The sign-on values.
    pub values: Values,
    /// Unix seconds.
    pub now: u64,
    /// `QSECURITY`.
    pub qsecurity: u8,
}

/// Binds of projected DNs against the profiles.
pub struct SecIdentity {
    /// The projected names.
    pub names: Names,
    /// `QSYS`.
    pub qsys: ObjectId,
    /// The system's values now.
    pub ctx: Rc<RefCell<BindCtx>>,
}

/// The profile name of a projected DN the identity owns.
fn name_of(dn: &Dn) -> String {
    dn.0.first()
        .and_then(|r| r.0.first())
        .map(|a| {
            String::from_utf8_lossy(&a.value)
                .trim()
                .to_ascii_uppercase()
        })
        .unwrap_or_default()
}

impl SecIdentity {
    /// The groups (projected DNs) and administrator flag of `user`.
    fn grant(&self, txn: &mut Txn<'_>, user: &str) -> Granted {
        let q = self.ctx.borrow().qsecurity;
        let p = Principal::load(txn, self.qsys, user, q).ok();
        let groups = p
            .as_ref()
            .map(|p| p.groups.iter().map(|g| self.names.profile_dn(g)).collect())
            .unwrap_or_default();
        let admin = p.as_ref().is_some_and(|p| {
            sec::fcnusg::check(txn, self.qsys, p, ADMIN_FUNCTION).unwrap_or(false)
        });
        Granted {
            groups,
            admin,
            warning: None,
        }
    }
}

impl Identity for SecIdentity {
    fn owns(&self, schema: &Schema, dn: &Dn) -> bool {
        self.names.profile_of(schema, dn).is_some()
    }

    fn bind(&mut self, txn: &mut Txn<'_>, dn: &Dn, password: &[u8]) -> Result<Granted, Refused> {
        let user = name_of(dn);
        let (values, now) = {
            let c = self.ctx.borrow();
            (c.values.clone(), c.now)
        };
        let attempt = Attempt {
            user: user.clone(),
            password: String::from_utf8_lossy(password).into_owned(),
            device: String::new(),
            console: false,
            sessions: 0,
            now,
            proof: None,
            ticket: false,
        };
        let refused = |policy| Refused {
            code: ResultCode::InvalidCredentials,
            policy,
        };
        match sec::signon::sign_on(txn, self.qsys, &attempt, &values, &|_, _| None) {
            Err(e) => {
                eprintln!("qsstg: directory bind of {user}: {e}");
                Err(Refused {
                    code: ResultCode::OperationsError,
                    policy: None,
                })
            }
            Ok(Err(r)) => Err(refused(
                matches!(r.msgid.as_str(), "CPF1393" | "CPF1394")
                    .then_some(PolicyError::AccountLocked),
            )),
            Ok(Ok(s)) if s.next == Next::ChangePassword => {
                Err(refused(Some(PolicyError::PasswordExpired)))
            }
            Ok(Ok(s)) => {
                let mut g = self.grant(txn, &s.user);
                g.warning = s.expires_in.map(|d| {
                    PolicyWarning::TimeBeforeExpiration(
                        i32::try_from(d.max(0).saturating_mul(86_400)).unwrap_or(i32::MAX),
                    )
                });
                Ok(g)
            }
        }
    }

    fn external(&mut self, txn: &mut Txn<'_>, dn: &Dn) -> Option<Granted> {
        let user = name_of(dn);
        let (_, p) = sec::profile::objects::read(txn, self.qsys, &user).ok()?;
        if p.disabled || p.pwd_none {
            return None;
        }
        Some(self.grant(txn, &user))
    }
}

/// What the audit hook knows of the connection in progress.
#[derive(Debug, Clone, Default)]
pub struct AuditCtx {
    /// The session's ID (`DICONN`).
    pub connection: String,
    /// The client's address.
    pub client: String,
    /// Under TLS.
    pub tls: bool,
    /// The switches in force.
    pub switches: AuditSwitches,
}

/// `DI` entries for an instance.
pub struct SecAudit {
    /// The instance.
    pub instance: String,
    /// The projected names (a bound profile is charged).
    pub names: Names,
    /// The request in progress.
    pub ctx: Rc<RefCell<AuditCtx>>,
}

/// The `DI` operation of an event.
fn di_op(ev: &AuditEvent) -> Op {
    let ok = matches!(
        ev.code,
        ResultCode::Success | ResultCode::CompareTrue | ResultCode::CompareFalse
    );
    if ev.op == AuditOp::Bind {
        return match ev.code {
            ResultCode::Success => Op::Bind,
            ResultCode::InvalidCredentials => Op::PasswordFail,
            _ => Op::AuthFail,
        };
    }
    if ev.code == ResultCode::InsufficientAccessRights {
        return Op::AuthFail;
    }
    let config = Dn::parse(&ev.target).is_ok_and(|d| {
        d.0.last().is_some_and(|r| {
            r.0.first().is_some_and(|a| {
                a.ty.eq_ignore_ascii_case("cn")
                    && String::from_utf8_lossy(&a.value).eq_ignore_ascii_case("configuration")
            })
        })
    });
    match ev.op {
        AuditOp::Add | AuditOp::Modify | AuditOp::Delete | AuditOp::ModDn if config && ok => {
            Op::ConfigChange
        }
        AuditOp::Add => Op::Create,
        AuditOp::Modify => Op::Change,
        AuditOp::Delete => Op::Delete,
        AuditOp::ModDn => Op::Rename,
        AuditOp::Extended if ev.target == PASSWORD_MODIFY => Op::PasswordChange,
        AuditOp::Bind | AuditOp::Search | AuditOp::Compare | AuditOp::Extended => Op::Read,
    }
}

fn request(op: AuditOp) -> &'static str {
    match op {
        AuditOp::Bind => "BIND",
        AuditOp::Search => "SEARCH",
        AuditOp::Compare => "COMPARE",
        AuditOp::Add => "ADD",
        AuditOp::Modify => "MODIFY",
        AuditOp::Delete => "DELETE",
        AuditOp::ModDn => "MODIFYDN",
        AuditOp::Extended => "EXTENDED",
    }
}

/// The profile a bound DN names, compared as the configuration compares
/// names (no schema at hand: `os400-profile` is the only type taken).
fn profile_named(names: &Names, who: &str) -> Option<String> {
    let d = Dn::parse(who).ok()?;
    let first = d.0.first()?.0.first()?;
    if !first.ty.eq_ignore_ascii_case("os400-profile") {
        return None;
    }
    let container = Dn::parse(&names.profile_dn("X")).ok()?.parent()?;
    let parent = d.parent()?;
    (dirsrv::config::dn_key(&parent.to_string()) == dirsrv::config::dn_key(&container.to_string()))
        .then(|| name_of(&d))
}

impl SecAudit {
    /// The fields of an entry for the session's connection.
    fn fields(&self, who: Option<&str>) -> DiFields {
        let c = self.ctx.borrow();
        let profile = who.and_then(|w| profile_named(&self.names, w));
        DiFields {
            instance: self.instance.clone(),
            connection: c.connection.clone(),
            client: c.client.clone(),
            ldap_version: "V3".into(),
            tls: c.tls,
            user_dn: who.unwrap_or_default().to_string(),
            profile,
            ..DiFields::default()
        }
    }

    /// An unbind (`UB`), when `ibm-auditUnbind` is on.
    pub fn unbind(&self, who: Option<&str>) {
        let s = self.ctx.borrow().switches;
        if s.on && s.unbind && !s.failed_only {
            sec::audit::di::audit_di(Op::Unbind, &self.fields(who));
        }
    }
}

impl Audit for SecAudit {
    fn record(&mut self, ev: &AuditEvent) {
        let ok = matches!(
            ev.code,
            ResultCode::Success | ResultCode::CompareTrue | ResultCode::CompareFalse
        );
        if !self.ctx.borrow().switches.wants(ev.op, ok) {
            return;
        }
        let op = di_op(ev);
        let mut f = self.fields(ev.who.as_deref());
        f.request = request(ev.op).into();
        f.object_dn.clone_from(&ev.target);
        if ev.op == AuditOp::Bind {
            f.auth = "SIMPLE".into();
            // A bind's identity is the name it gave.
            if ok {
                f.user_dn.clone_from(&ev.target);
            }
        }
        if !ok {
            f.fail_code = ev.code.number().to_string();
        }
        sec::audit::di::audit_di(op, &f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(op: AuditOp, target: &str, code: ResultCode) -> AuditEvent {
        AuditEvent {
            op,
            target: target.into(),
            who: None,
            code,
        }
    }

    #[test]
    fn operations_map_to_di_codes() {
        use AuditOp as A;
        use ResultCode as R;
        let cases = [
            (ev(A::Bind, "cn=a", R::Success), Op::Bind),
            (ev(A::Bind, "cn=a", R::InvalidCredentials), Op::PasswordFail),
            (
                ev(A::Bind, "cn=a", R::InappropriateAuthentication),
                Op::AuthFail,
            ),
            (
                ev(A::Modify, "cn=a", R::InsufficientAccessRights),
                Op::AuthFail,
            ),
            (ev(A::Add, "cn=a,o=x", R::Success), Op::Create),
            (
                ev(A::Modify, "cn=SSL, cn=Configuration", R::Success),
                Op::ConfigChange,
            ),
            (ev(A::Delete, "cn=a", R::Success), Op::Delete),
            (ev(A::ModDn, "cn=a", R::Success), Op::Rename),
            (ev(A::Search, "o=x", R::Success), Op::Read),
            (ev(A::Compare, "o=x", R::CompareFalse), Op::Read),
            (
                ev(A::Extended, PASSWORD_MODIFY, R::Success),
                Op::PasswordChange,
            ),
            (
                ev(A::Extended, "1.3.6.1.4.1.4203.1.11.3", R::Success),
                Op::Read,
            ),
        ];
        for (e, want) in cases {
            assert_eq!(di_op(&e), want, "{e:?}");
        }
    }
}
