// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The certificate calls between the `ADMIN` job and the storage
//! service, which alone opens the stores: [`CertCall`] and
//! [`CertReply`] as bytes, [`answer`] on the service's side and
//! [`Remote`], the job's [`Certs`]. A call that changes something is
//! sent as the signed-on user, so the service checks that user's
//! authority. Calls carry store passwords: neither type is `Debug`, and
//! nothing here logs.

use serde::{Deserialize, Serialize};

use super::Manager;
use crate::env::{AppInfo, CertDetail, CertFailure, CertRow, Certs, Import, ImportSource, NewCert, StoreInfo};

/// One call.
#[derive(Clone, Serialize, Deserialize)]
pub enum CertCall {
    /// `stores`.
    Stores,
    /// `entries`.
    Entries {
        /// The store.
        store: String,
    },
    /// `detail`.
    Detail {
        /// The store.
        store: String,
        /// The label.
        label: String,
    },
    /// `export`.
    Export {
        /// The store.
        store: String,
        /// The label.
        label: String,
        /// With its issuers.
        chain: bool,
    },
    /// `create_store`.
    CreateStore {
        /// The name.
        name: String,
        /// The path.
        path: String,
        /// The password.
        password: String,
    },
    /// `change_password`.
    ChangePassword {
        /// The store.
        store: String,
        /// The password now.
        old: String,
        /// The new one.
        new: String,
    },
    /// `delete_store`.
    DeleteStore {
        /// The store.
        store: String,
    },
    /// `signers`.
    Signers,
    /// `create`.
    Create(NewCert),
    /// `import`, the file's bytes read.
    Import(Import),
    /// `set_default`.
    SetDefault {
        /// The store.
        store: String,
        /// The label.
        label: String,
    },
    /// `remove`.
    Remove {
        /// The store.
        store: String,
        /// The label.
        label: String,
    },
    /// `apps`.
    Apps,
    /// `assign`.
    Assign {
        /// The application ID.
        app: String,
        /// The label, `None` to remove the assignment.
        label: Option<String>,
    },
    /// `register_instance`.
    RegisterInstance {
        /// The instance.
        instance: String,
    },
    /// `deregister_instance`.
    DeregisterInstance {
        /// The instance.
        instance: String,
    },
    /// `copy`.
    Copy {
        /// The store it is in.
        from: String,
        /// The label.
        label: String,
        /// The store it goes to.
        to: String,
    },
    /// `export_pkcs12`.
    ExportPkcs12 {
        /// The store.
        store: String,
        /// The label.
        label: String,
        /// The file's password.
        password: String,
    },
}

impl CertCall {
    /// Whether it changes stores or gives a private key away (needs
    /// `*ALLOBJ` and `*SECADM`; sent as the user).
    pub fn changes(&self) -> bool {
        !matches!(
            self,
            CertCall::Stores
                | CertCall::Entries { .. }
                | CertCall::Detail { .. }
                | CertCall::Export { .. }
                | CertCall::Signers
                | CertCall::Apps
        )
    }

    /// Whether the administration server sends it as itself: the
    /// instances' application IDs follow the instances, which any user
    /// with `*IOSYSCFG` creates and deletes.
    pub fn by_server(&self) -> bool {
        matches!(self, CertCall::RegisterInstance { .. } | CertCall::DeregisterInstance { .. })
    }

    /// The bytes sent.
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// From the bytes sent.
    pub fn decode(b: &[u8]) -> Option<Self> {
        serde_json::from_slice(b).ok()
    }
}

/// One answer.
#[derive(Clone, Serialize, Deserialize)]
pub enum CertReply {
    /// The stores.
    Stores(Vec<StoreInfo>),
    /// Entries.
    Rows(Result<Vec<CertRow>, CertFailure>),
    /// One entry.
    Detail(Result<CertDetail, CertFailure>),
    /// A file.
    Bytes(Result<Vec<u8>, CertFailure>),
    /// Done.
    Done(Result<(), CertFailure>),
    /// Names (signers).
    Names(Vec<String>),
    /// Labels added.
    Labels(Result<Vec<String>, CertFailure>),
    /// Application IDs.
    Apps(Vec<AppInfo>),
    /// The call was refused before it ran.
    Refused(CertFailure),
}

impl CertReply {
    /// The bytes sent back.
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// From the bytes sent back.
    pub fn decode(b: &[u8]) -> Option<Self> {
        serde_json::from_slice(b).ok()
    }
}

/// Run `call` on `m`.
pub fn answer(m: &Manager<'_>, call: CertCall) -> CertReply {
    match call {
        CertCall::Stores => CertReply::Stores(m.stores()),
        CertCall::Entries { store } => CertReply::Rows(m.entries(&store)),
        CertCall::Detail { store, label } => CertReply::Detail(m.detail(&store, &label)),
        CertCall::Export { store, label, chain } => CertReply::Bytes(m.export(&store, &label, chain)),
        CertCall::CreateStore { name, path, password } => CertReply::Done(m.create_store(&name, &path, &password)),
        CertCall::ChangePassword { store, old, new } => CertReply::Done(m.change_password(&store, &old, &new)),
        CertCall::DeleteStore { store } => CertReply::Done(m.delete_store(&store)),
        CertCall::Signers => CertReply::Names(m.signer_names()),
        CertCall::Create(n) => CertReply::Detail(m.create(&n)),
        CertCall::Import(i) => CertReply::Labels(m.import(&i)),
        CertCall::SetDefault { store, label } => CertReply::Done(m.set_default(&store, &label)),
        CertCall::Remove { store, label } => CertReply::Done(m.remove(&store, &label)),
        CertCall::Apps => CertReply::Apps(m.apps()),
        CertCall::Assign { app, label } => CertReply::Done(m.assign(&app, label.as_deref())),
        CertCall::RegisterInstance { instance } => CertReply::Done(m.register_instance(&instance)),
        CertCall::DeregisterInstance { instance } => CertReply::Done(m.deregister_instance(&instance)),
        CertCall::Copy { from, label, to } => CertReply::Done(m.copy(&from, &label, &to)),
        CertCall::ExportPkcs12 { store, label, password } => {
            CertReply::Bytes(m.export_pkcs12(&store, &label, &password))
        }
    }
}

/// How the job reaches the service.
pub trait Transport: Send + Sync {
    /// Send an encoded call, as `user` when given (else as the server
    /// itself); the encoded answer, `None` when none came.
    fn call(&self, user: Option<&str>, call: Vec<u8>) -> Option<Vec<u8>>;
    /// A stream file read with `user`'s authority.
    fn read_as(&self, user: &str, path: &str) -> Result<Vec<u8>, String>;
}

/// [`Certs`] through a [`Transport`].
pub struct Remote<T: Transport>(pub T);

fn silent() -> CertFailure {
    CertFailure::Invalid("the certificate service did not answer".into())
}

impl<T: Transport> Remote<T> {
    fn ask(&self, user: Option<&str>, call: &CertCall) -> Option<CertReply> {
        CertReply::decode(&self.0.call(user, call.encode())?)
    }

    fn done(&self, user: &str, call: &CertCall) -> Result<(), CertFailure> {
        self.done_as(Some(user), call)
    }

    fn server_done(&self, call: &CertCall) -> Result<(), CertFailure> {
        self.done_as(None, call)
    }

    fn done_as(&self, user: Option<&str>, call: &CertCall) -> Result<(), CertFailure> {
        match self.ask(user, call) {
            Some(CertReply::Done(r)) => r,
            Some(CertReply::Refused(f)) => Err(f),
            _ => Err(silent()),
        }
    }
}

impl<T: Transport> Certs for Remote<T> {
    fn stores(&self) -> Vec<StoreInfo> {
        match self.ask(None, &CertCall::Stores) {
            Some(CertReply::Stores(s)) => s,
            _ => Vec::new(),
        }
    }

    fn entries(&self, store: &str) -> Result<Vec<CertRow>, CertFailure> {
        match self.ask(None, &CertCall::Entries { store: store.into() }) {
            Some(CertReply::Rows(r)) => r,
            Some(CertReply::Refused(f)) => Err(f),
            _ => Err(silent()),
        }
    }

    fn detail(&self, store: &str, label: &str) -> Result<CertDetail, CertFailure> {
        let c = CertCall::Detail {
            store: store.into(),
            label: label.into(),
        };
        match self.ask(None, &c) {
            Some(CertReply::Detail(r)) => r,
            Some(CertReply::Refused(f)) => Err(f),
            _ => Err(silent()),
        }
    }

    fn export(&self, store: &str, label: &str, chain: bool) -> Result<Vec<u8>, CertFailure> {
        let c = CertCall::Export {
            store: store.into(),
            label: label.into(),
            chain,
        };
        match self.ask(None, &c) {
            Some(CertReply::Bytes(r)) => r,
            Some(CertReply::Refused(f)) => Err(f),
            _ => Err(silent()),
        }
    }

    fn create_store(&self, user: &str, name: &str, path: &str, password: &str) -> Result<(), CertFailure> {
        let c = CertCall::CreateStore {
            name: name.into(),
            path: path.into(),
            password: password.into(),
        };
        self.done(user, &c)
    }

    fn change_password(&self, user: &str, store: &str, old: &str, new: &str) -> Result<(), CertFailure> {
        let c = CertCall::ChangePassword {
            store: store.into(),
            old: old.into(),
            new: new.into(),
        };
        self.done(user, &c)
    }

    fn delete_store(&self, user: &str, store: &str) -> Result<(), CertFailure> {
        self.done(user, &CertCall::DeleteStore { store: store.into() })
    }

    fn signers(&self) -> Vec<String> {
        match self.ask(None, &CertCall::Signers) {
            Some(CertReply::Names(n)) => n,
            _ => Vec::new(),
        }
    }

    fn create(&self, user: &str, new: &NewCert) -> Result<CertDetail, CertFailure> {
        match self.ask(Some(user), &CertCall::Create(new.clone())) {
            Some(CertReply::Detail(r)) => r,
            Some(CertReply::Refused(f)) => Err(f),
            _ => Err(silent()),
        }
    }

    fn import(&self, user: &str, import: &Import) -> Result<Vec<String>, CertFailure> {
        let mut i = import.clone();
        if let ImportSource::Path(p) = &import.source {
            let data = self
                .0
                .read_as(user, p)
                .map_err(|e| CertFailure::Invalid(format!("stream file {p} could not be read: {e}")))?;
            i.source = ImportSource::Data(data);
        }
        match self.ask(Some(user), &CertCall::Import(i)) {
            Some(CertReply::Labels(r)) => r,
            Some(CertReply::Refused(f)) => Err(f),
            _ => Err(silent()),
        }
    }

    fn set_default(&self, user: &str, store: &str, label: &str) -> Result<(), CertFailure> {
        let c = CertCall::SetDefault {
            store: store.into(),
            label: label.into(),
        };
        self.done(user, &c)
    }

    fn remove(&self, user: &str, store: &str, label: &str) -> Result<(), CertFailure> {
        let c = CertCall::Remove {
            store: store.into(),
            label: label.into(),
        };
        self.done(user, &c)
    }

    fn apps(&self) -> Vec<AppInfo> {
        match self.ask(None, &CertCall::Apps) {
            Some(CertReply::Apps(a)) => a,
            _ => Vec::new(),
        }
    }

    fn assign(&self, user: &str, app: &str, label: Option<&str>) -> Result<(), CertFailure> {
        let c = CertCall::Assign {
            app: app.into(),
            label: label.map(String::from),
        };
        self.done(user, &c)
    }

    fn register_instance(&self, instance: &str) -> Result<(), CertFailure> {
        self.server_done(&CertCall::RegisterInstance { instance: instance.into() })
    }

    fn deregister_instance(&self, instance: &str) -> Result<(), CertFailure> {
        self.server_done(&CertCall::DeregisterInstance { instance: instance.into() })
    }

    fn copy(&self, user: &str, from: &str, label: &str, to: &str) -> Result<(), CertFailure> {
        let c = CertCall::Copy {
            from: from.into(),
            label: label.into(),
            to: to.into(),
        };
        self.done(user, &c)
    }

    fn export_pkcs12(&self, user: &str, store: &str, label: &str, password: &str) -> Result<Vec<u8>, CertFailure> {
        let c = CertCall::ExportPkcs12 {
            store: store.into(),
            label: label.into(),
            password: password.into(),
        };
        match self.ask(Some(user), &c) {
            Some(CertReply::Bytes(r)) => r,
            Some(CertReply::Refused(f)) => Err(f),
            _ => Err(silent()),
        }
    }
}
