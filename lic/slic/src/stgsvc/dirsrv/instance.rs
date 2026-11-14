// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One directory server instance in the service (docs/DIRSRV.md §4,
//! §10): its `*DIRDB` (`QUSRDIRDB/NAME`, made at the first open), its
//! schema (shipped, then `etc/V3.modifiedschema`), its configuration
//! file and the [`Directory`] over them with the configuration backend,
//! the projected backend and the hooks.
//!
//! The configuration backend keeps the file's text in memory; a modify
//! through `cn=configuration` changes it there and marks it changed, and
//! the service writes it after the request ([`Instance::flush`]), then
//! applies what takes effect at once (the administrator's password,
//! limits, auditing: [`Instance::apply`]).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use dirsrv::backend::config::{ConfigBackend, ConfigSource};
use dirsrv::backend::projected::{Names, ProjectedBackend};
use dirsrv::config::settings::{self as conf_settings, Runtime};
use dirsrv::config::{ConfigError, InstanceList, SlapdConf};
use dirsrv::ops::{Directory, SystemEnv};
use dirsrv::schema::Schema;
use dirsrv::store::{CreateOpts, DirDb, Normalizer};
use ldap::ResultCode;
use ldap::msg::{Attribute, ModOp, Modification};
use qsname::Name;
use stg::dir::DirError;
use store::obj::{ObjectId, ops};
use store::txn::Txn;

use super::hooks::{AuditCtx, BindCtx, SecAudit, SecIdentity};
use super::profiles::SecProfiles;

/// The library of the entry stores.
pub const LIBRARY: &str = "QUSRDIRDB";
/// The profile the instance jobs run as.
pub const USER: &str = "QDIRSRV";

/// What an instance asks of the system: files, the store, values.
pub trait Host {
    /// A file's text, `None` when it does not exist.
    ///
    /// # Errors
    /// The IFS's.
    fn read(&mut self, path: &str) -> Result<Option<String>, String>;
    /// Replace a configuration file (owner-only, `QDIRSRV`'s).
    ///
    /// # Errors
    /// The IFS's.
    fn write(&mut self, path: &str, text: &str) -> Result<(), String>;
    /// The instance list.
    ///
    /// # Errors
    /// `GLD0127`.
    fn instances(&mut self) -> Result<InstanceList, ConfigError>;
    /// The sign-on values and the clock, for profile binds.
    fn bind_ctx(&self) -> BindCtx;
    /// Run `f` with a new store transaction (its to commit) and `QSYS`;
    /// `None` without a store.
    fn with_store<T>(&mut self, f: impl FnOnce(Txn<'_>, ObjectId) -> T) -> Option<T>;
}

/// A refusal as the job logs it.
pub fn refused(e: &ConfigError) -> DirError {
    let vars = e
        .vars()
        .into_iter()
        .map(|v| match v {
            dirsrv::config::Var::Chars(s, _) | dirsrv::config::Var::Vary(s) => s,
        })
        .collect();
    DirError::new(e.msgid(), vars, e.to_string())
}

/// A failure of the service's own (`CPF9898`).
pub fn failed(text: impl Into<String>) -> DirError {
    let text = text.into();
    DirError::new("CPF9898", vec![text.clone()], text)
}

/// Message `msgid` of `QGLDMSG` with `vars`; its text is the message
/// file's, filled in as the answer leaves the service
/// ([`super::serve`]).
pub fn dir_msg(msgid: &str, vars: Vec<String>) -> DirError {
    DirError::new(msgid, vars, String::new())
}

/// `GLD0128`: no object store.
pub fn no_store() -> DirError {
    dir_msg(NO_STORE, Vec::new())
}

/// The directory server cannot reach the object store.
pub const NO_STORE: &str = "GLD0128";
/// The entry store could not be used: the reason.
pub const STORE_FAILED: &str = "GLD014D";

/// A missing file's reason, as the IFS words it.
fn missing() -> String {
    ifs::Errno::ENOENT.text().into()
}

/// The configuration file as the backend and the service share it.
pub struct ConfState {
    /// The file.
    pub conf: SlapdConf,
    /// The instance.
    pub instance: String,
    /// Changed since it was last written.
    pub changed: bool,
}

/// `cn=configuration` over [`ConfState`].
pub struct ConfFile(pub Arc<Mutex<ConfState>>);

const PASSWORDS: [&str; 2] = ["ibm-slapdAdminPW", "ibm-slapdMasterPW"];

impl ConfigSource for ConfFile {
    fn entries(&self) -> Vec<(String, Vec<Attribute>)> {
        let st = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        st.conf
            .entries()
            .map(|(dn, lines)| {
                let mut attrs: Vec<Attribute> = Vec::new();
                for (ty, v) in lines {
                    match attrs.iter_mut().find(|a| a.ty.eq_ignore_ascii_case(ty)) {
                        Some(a) => a.vals.push(v.into_bytes()),
                        None => attrs.push(Attribute::new(ty, &[v.as_str()])),
                    }
                }
                (dn.to_string(), attrs)
            })
            .collect()
    }

    fn modify(&mut self, dn: &str, mods: &[Modification]) -> Result<(), (ResultCode, String)> {
        let mut st = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut conf = st.conf.clone();
        for m in mods {
            let ty = m.attr.ty.as_str();
            let given: Vec<String> = m
                .attr
                .vals
                .iter()
                .map(|v| String::from_utf8_lossy(v).into_owned())
                .collect();
            let mut vals = conf.values(dn, ty);
            match m.op {
                ModOp::Add => vals.extend(given),
                ModOp::Replace => vals = given,
                ModOp::Delete if given.is_empty() => vals.clear(),
                ModOp::Delete => vals.retain(|v| !given.iter().any(|g| g.trim() == v.trim())),
                // ConfigBackend refuses an increment with its text first.
                ModOp::Increment => return Err((ResultCode::UnwillingToPerform, String::new())),
            }
            // A password given in the clear is kept hashed, as CHGDIRSVRA keeps it.
            if PASSWORDS.iter().any(|p| p.eq_ignore_ascii_case(ty)) {
                for v in &mut vals {
                    if !dirsrv::pwd::scheme::is_hashed(v.as_bytes()) {
                        *v = dirsrv::pwd::hash(v.as_bytes());
                    }
                }
            }
            let refs: Vec<&str> = vals.iter().map(String::as_str).collect();
            conf.set(dn, ty, &refs)
                .map_err(|e| (ResultCode::UnwillingToPerform, e.to_string()))?;
        }
        conf_settings::runtime(&conf, &st.instance, &release::RELEASE.to_string())
            .map_err(|e| (ResultCode::UnwillingToPerform, e.to_string()))?;
        st.conf = conf;
        st.changed = true;
        Ok(())
    }
}

/// One instance.
pub struct Instance {
    /// Its name.
    pub name: String,
    /// The operations.
    pub dir: Directory,
    /// The configuration as last read.
    pub runtime: Runtime,
    /// `etc/V3.modifiedschema`.
    pub schema_path: String,
    conf: Arc<Mutex<ConfState>>,
    bind: Rc<RefCell<BindCtx>>,
    audit: Rc<RefCell<AuditCtx>>,
    unbinds: SecAudit,
}

fn store_err(e: impl std::fmt::Display) -> DirError {
    dir_msg(STORE_FAILED, vec![e.to_string()])
}

/// A server ID for a file without one: 16 random bytes as a UUID.
fn server_id() -> String {
    let mut b = [0u8; 16];
    dirsrv::ops::Env::random(&SystemEnv, &mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().fold(String::new(), |mut acc, x| {
        use std::fmt::Write as _;
        let _ = write!(acc, "{x:02x}");
        acc
    });
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// The instance's `V3.modifiedschema`: `ibm-slapdSchemaAdditions`, or
/// `etc/` of its `idsslapd-NAME`.
pub fn schema_path(conf: &SlapdConf, inst: &dirsrv::config::Instance) -> String {
    conf.get(conf_settings::SCHEMA, "ibm-slapdSchemaAdditions")
        .map_or_else(
            || {
                format!(
                    "{}/idsslapd-{}/etc/V3.modifiedschema",
                    inst.location.trim_end_matches('/'),
                    inst.name
                )
            },
            |p| p.trim().to_string(),
        )
}

/// The shipped schema with the additions at `path`.
///
/// # Errors
/// The IFS's; `GLD0127` for additions that do not read.
pub fn load_schema(host: &mut impl Host, path: &str) -> Result<Schema, DirError> {
    let mut schema = Schema::shipped().map_err(|e| failed(e.to_string()))?;
    if let Some(t) = host.read(path).map_err(failed)? {
        schema.load_modified(&t).map_err(|e| {
            refused(&ConfigError::File {
                file: path.to_string(),
                why: e.to_string(),
            })
        })?;
    }
    Ok(schema)
}

/// The store of instance `name`, made when missing with the instance's
/// description as its text.
fn dirdb(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    name: &str,
    text: &str,
    now: u64,
) -> Result<DirDb, DirError> {
    let lib = ops::find_library(txn, qsys, Name::parse(LIBRARY).expect("valid"))
        .map_err(|e| failed(format!("{LIBRARY}: {e}")))?
        .id;
    let n = Name::parse(name).map_err(store_err)?;
    if let Some(db) = DirDb::open(txn, lib, n).map_err(store_err)? {
        return Ok(db);
    }
    DirDb::create(
        txn,
        lib,
        n,
        &CreateOpts {
            text: text.into(),
            owner: USER.into(),
            system: "QS400".into(),
            now,
        },
    )
    .map_err(store_err)
}

/// Make the configured indexes exist and drop the configured ones no
/// longer listed (the ones always kept stay).
fn indexes(txn: &mut Txn<'_>, db: DirDb, schema: &Schema, want: &[String]) -> Result<(), DirError> {
    let mut ids = Vec::new();
    for a in want {
        let id = schema.attr_id(a).ok_or_else(|| {
            refused(&ConfigError::BadValue {
                attr: "ibm-slapdIndex".into(),
                value: a.clone(),
            })
        })?;
        ids.push(id);
    }
    let defs = db.indexes(txn).map_err(store_err)?;
    for d in &defs {
        if !d.always && !ids.contains(&d.attr) {
            db.remove_index(txn, d.number).map_err(store_err)?;
        }
    }
    for id in ids {
        if !defs.iter().any(|d| d.attr == id) {
            db.add_index(txn, schema, &id, 0x07).map_err(store_err)?;
        }
    }
    Ok(())
}

impl Instance {
    /// Open instance `name`: its file read and checked, its store opened
    /// or made, its schema loaded, the directory built and seeded.
    ///
    /// # Errors
    /// `GLD0203` for no such instance; the file's `GLD0126`, `GLD0127`,
    /// `GLD0232`; the store's.
    #[allow(clippy::too_many_lines)]
    pub fn open(host: &mut impl Host, name: &str) -> Result<Self, DirError> {
        let list = host.instances().map_err(|e| refused(&e))?;
        let inst = list
            .find(name)
            .ok_or_else(|| refused(&ConfigError::NoServerInstance(name.into())))?;
        let name = inst.name.clone();
        let desc = inst.desc.clone();
        let path = inst.conf_path();
        let text = host
            .read(&path)
            .map_err(|why| {
                refused(&ConfigError::File {
                    file: path.clone(),
                    why,
                })
            })?
            .ok_or_else(|| {
                refused(&ConfigError::File {
                    file: path.clone(),
                    why: missing(),
                })
            })?;
        let mut conf = SlapdConf::parse(&path, &text).map_err(|e| refused(&e))?;
        if conf
            .get(dirsrv::config::slapd::CONFIGURATION, "ibm-slapdServerId")
            .is_none_or(|v| v.trim().is_empty())
        {
            conf.set(
                dirsrv::config::slapd::CONFIGURATION,
                "ibm-slapdServerId",
                &[&server_id()],
            )
            .map_err(|e| refused(&e))?;
            host.write(&path, conf.text()).map_err(failed)?;
        }
        let runtime = conf_settings::runtime(&conf, &name, &release::RELEASE.to_string())
            .map_err(|e| refused(&e))?;
        let schema_path = schema_path(&conf, inst);
        let schema = load_schema(host, &schema_path)?;
        let bind = Rc::new(RefCell::new(host.bind_ctx()));
        let audit = Rc::new(RefCell::new(AuditCtx {
            switches: runtime.audit,
            ..AuditCtx::default()
        }));
        let conf = Arc::new(Mutex::new(ConfState {
            conf,
            instance: name.clone(),
            changed: false,
        }));
        let names = runtime
            .projected
            .as_deref()
            .map(Names::new)
            .transpose()
            .map_err(|e| failed(e.message))?;
        let now = bind.borrow().now;
        let qsecurity = bind.borrow().qsecurity;
        let settings = runtime.settings.clone();
        let built = host.with_store(|mut t, qsys| -> Result<Directory, DirError> {
            let txn = &mut t;
            let db = dirdb(txn, qsys, &name, &desc, now)?;
            let mut dir = Directory::new(txn, db, schema, settings, Box::new(SystemEnv))
                .map_err(|e| failed(e.message))?;
            dir.seed(txn).map_err(|e| failed(e.message))?;
            indexes(txn, db, &dir.schema, &runtime.indexes)?;
            dir.add_backend(Box::new(ConfigBackend::new(ConfFile(Arc::clone(&conf)))));
            if let (Some(n), Some(p)) = (&names, &runtime.projected) {
                let b = ProjectedBackend::new(p, SecProfiles { qsys, qsecurity })
                    .map_err(|e| failed(e.message))?;
                dir.add_backend(Box::new(b));
                dir.set_identity(Box::new(SecIdentity {
                    names: n.clone(),
                    qsys,
                    ctx: Rc::clone(&bind),
                }));
            }
            t.commit().map_err(store_err)?;
            Ok(dir)
        });
        let mut dir = built.ok_or_else(no_store)??;
        let names_or_none = names
            .clone()
            .unwrap_or_else(|| Names::new("os400-sys=SYSTEM").expect("a DN"));
        dir.set_audit(Box::new(SecAudit {
            instance: name.clone(),
            names: names_or_none.clone(),
            ctx: Rc::clone(&audit),
        }));
        let unbinds = SecAudit {
            instance: name.clone(),
            names: names_or_none,
            ctx: Rc::clone(&audit),
        };
        Ok(Self {
            name,
            dir,
            runtime,
            schema_path,
            conf,
            bind,
            audit,
            unbinds,
        })
    }

    /// Before a request: the system's values now and the connection's
    /// facts for auditing.
    pub fn prepare(&self, host: &impl Host, connection: u64, client: &str, tls: bool) {
        *self.bind.borrow_mut() = host.bind_ctx();
        let mut a = self.audit.borrow_mut();
        a.connection = connection.to_string();
        a.client = client.to_string();
        a.tls = tls;
    }

    /// An unbind for the audit journal.
    pub fn unbind(&self, who: Option<&str>) {
        self.unbinds.unbind(who);
    }

    /// The configuration file's path.
    fn conf_path(&self) -> String {
        self.conf
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .conf
            .path()
            .to_string()
    }

    /// After a request: write the configuration file when
    /// `cn=configuration` changed it, and apply it.
    ///
    /// # Errors
    /// The IFS's.
    pub fn flush(&mut self, host: &mut impl Host) -> Result<(), DirError> {
        let text = {
            let mut st = self.conf.lock().unwrap_or_else(PoisonError::into_inner);
            if !st.changed {
                return Ok(());
            }
            st.changed = false;
            st.conf.text().to_string()
        };
        host.write(&self.conf_path(), &text).map_err(failed)?;
        self.apply_text(&text)
    }

    /// The file changed outside (`CHGDIRSVRA`, `dir_admin`): read it
    /// again and apply it.
    ///
    /// # Errors
    /// The IFS's, or a file that does not check.
    pub fn reload(&mut self, host: &mut impl Host) -> Result<(), DirError> {
        let path = self.conf_path();
        let text = host.read(&path).map_err(failed)?.ok_or_else(|| {
            refused(&ConfigError::File {
                file: path.clone(),
                why: missing(),
            })
        })?;
        self.apply_text(&text)
    }

    fn apply_text(&mut self, text: &str) -> Result<(), DirError> {
        let path = self.conf_path();
        let conf = SlapdConf::parse(&path, text).map_err(|e| refused(&e))?;
        let rt = conf_settings::runtime(&conf, &self.name, &release::RELEASE.to_string())
            .map_err(|e| refused(&e))?;
        self.apply(&rt);
        self.conf
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .conf = conf;
        Ok(())
    }

    /// What takes effect at once (§10): the administrator's password,
    /// the limits, the audit switches. The rest waits for the next start.
    pub fn apply(&mut self, rt: &Runtime) {
        let s = &mut self.dir.settings;
        s.admin_pw.clone_from(&rt.settings.admin_pw);
        s.size_limit = rt.settings.size_limit;
        s.time_limit = rt.settings.time_limit;
        self.audit.borrow_mut().switches = rt.audit;
        self.runtime
            .settings
            .admin_pw
            .clone_from(&rt.settings.admin_pw);
        self.runtime.settings.size_limit = rt.settings.size_limit;
        self.runtime.settings.time_limit = rt.settings.time_limit;
        self.runtime.audit = rt.audit;
    }
}
