// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The areas not live yet: their pages show empty lists. A command still
//! runs its command as the user.

use navigator::env::db::{Db, DbObject, ObjKind, Schema, SqlOptions, SqlOutcome, TableDesc};
use navigator::env::sec::{AuditEntry, AuditFilter, AuditReceiver, Security};
use navigator::env::sw::{Fix, ImageCatalog, LicenseKey, Product, Ptf, PtfGroup, Software, UpdateAttrs};
use qsweb::msg::{Kind, Message};

use super::Link;

/// The areas not live yet.
pub struct Pending(pub Link);

fn not_yet(what: &str) -> Message {
    Message::new(Kind::Error, "CPF9899", &format!("{what} is not available from Navigator yet."))
}

/// The database, the audit journal and software: nothing read yet
/// (TODO: live over the `QSYS2` catalog views, `isql_run`,
/// `DSPAUDJRNE`, `licpgm_panel`, the PTF database).
impl Db for Pending {
    fn schemas(&self, _user: &str) -> Vec<Schema> {
        Vec::new()
    }

    fn sql_schemas(&self, _user: &str) -> Vec<String> {
        Vec::new()
    }

    fn objects(&self, _user: &str, _schema: &str, _kind: ObjKind) -> Vec<DbObject> {
        Vec::new()
    }

    fn table(&self, _user: &str, _schema: &str, _table: &str) -> Option<TableDesc> {
        None
    }

    fn ddl(&self, _user: &str, _schema: &str, _table: &str) -> Result<String, Message> {
        Err(not_yet("Generate SQL"))
    }

    fn run_sql(&self, _user: &str, _statement: &str, _opts: &SqlOptions, _max_rows: Option<usize>) -> SqlOutcome {
        SqlOutcome {
            messages: vec![not_yet("Run SQL")],
            ..SqlOutcome::default()
        }
    }

    fn explain(&self, _user: &str, _statement: &str, _opts: &SqlOptions) -> Result<Vec<String>, Message> {
        Err(not_yet("Explain"))
    }
}

impl Security for Pending {
    fn audit_entries(&self, _user: &str, _filter: &AuditFilter) -> Result<Vec<AuditEntry>, Message> {
        Ok(Vec::new())
    }

    fn audit_receiver(&self, _user: &str) -> AuditReceiver {
        AuditReceiver::default()
    }
}

impl Software for Pending {
    fn products(&self, _user: &str) -> Vec<Product> {
        Vec::new()
    }

    fn catalogs(&self, _user: &str) -> Vec<ImageCatalog> {
        Vec::new()
    }

    fn catalog_products(&self, _user: &str, _catalog: &str) -> Result<Vec<Product>, Message> {
        Ok(Vec::new())
    }

    fn license_keys(&self, _user: &str) -> Vec<LicenseKey> {
        Vec::new()
    }

    fn ptfs(&self, _user: &str) -> Vec<Ptf> {
        Vec::new()
    }

    fn ptf_groups(&self, _user: &str) -> Vec<PtfGroup> {
        Vec::new()
    }

    fn cover_letter(&self, _user: &str, _id: &str) -> Option<String> {
        None
    }

    fn fixes(&self, _user: &str) -> Vec<Fix> {
        Vec::new()
    }

    fn update_attrs(&self, _user: &str) -> UpdateAttrs {
        UpdateAttrs::default()
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.0.run(user, command)
    }
}
