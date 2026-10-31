// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped catalog: `catalog.toml`, embedded, with the agreement
//! texts of `agreements/`.

use std::sync::OnceLock;

use crate::id::{LoadId, LoadType, OptionNo};
use crate::model::QualName;
use crate::model::state::LoadState;
use crate::pkg::{self, Package, Status};
use crate::store::{ProductStore, StoreError};

const CATALOG: &str = include_str!("../catalog.toml");

/// Every product's agreement text, by product ID.
pub const AGREEMENTS: &[(&str, &str)] = &[
    ("QS00SS1", include_str!("../agreements/QS00SS1.txt")),
    ("QS00WDS", include_str!("../agreements/QS00WDS.txt")),
    ("QS00TC1", include_str!("../agreements/QS00TC1.txt")),
    ("QS00ST1", include_str!("../agreements/QS00ST1.txt")),
    ("QS00DG1", include_str!("../agreements/QS00DG1.txt")),
    ("QS00NAV", include_str!("../agreements/QS00NAV.txt")),
    ("QS00AFW", include_str!("../agreements/QS00AFW.txt")),
    ("QS00PSE", include_str!("../agreements/QS00PSE.txt")),
];

/// The shipped products.
#[derive(Debug)]
pub struct Catalog {
    /// One package per product, the operating system first.
    pub products: Vec<Package>,
}

impl Catalog {
    /// The product with an ID (or its IBM alias).
    #[must_use]
    pub fn product(&self, id: &str) -> Option<&Package> {
        self.products.iter().find(|p| p.definition.answers_to(id))
    }

    /// The products shipped with the system.
    pub fn shipped(&self) -> impl Iterator<Item = &Package> {
        self.products.iter().filter(|p| p.status == Status::Shipped)
    }

    /// The products whose IDs are reserved, not installed.
    pub fn reserved(&self) -> impl Iterator<Item = &Package> {
        self.products
            .iter()
            .filter(|p| p.status == Status::Reserved)
    }
}

/// The catalog, parsed once.
///
/// # Panics
/// When the embedded catalog does not parse; a unit test keeps it valid.
#[must_use]
pub fn shipped() -> &'static Catalog {
    static CAT: OnceLock<Catalog> = OnceLock::new();
    CAT.get_or_init(|| parse(CATALOG).expect("catalog.toml is valid"))
}

/// The agreement text of a product.
#[must_use]
pub fn agreement_text(id: &str) -> Option<&'static str> {
    AGREEMENTS.iter().find(|(p, _)| *p == id).map(|(_, t)| *t)
}

/// First IPL: create every catalog product's `*PRDDFN` in `QSYS`, the
/// shipped products' loads there in state `90` (installed at `now`,
/// seconds since the epoch) and their license information. Reserved
/// products get their definition only. Objects already there are left
/// alone.
pub fn seed(store: &mut dyn ProductStore, now: u64) -> Result<(), StoreError> {
    let cat = shipped();
    let mut lic = store.licenses();
    for p in &cat.products {
        let dq = QualName::new(&p.definition_name.name, "QSYS");
        match store.create_definition(&dq, &p.definition) {
            Ok(()) | Err(StoreError::Exists(_)) => {}
            Err(e) => return Err(e),
        }
        if p.status != Status::Shipped {
            continue;
        }
        for (q, l) in &p.loads {
            let mut l = l.clone();
            l.state = LoadState::Installed;
            l.installed_at = now;
            match store.create_load(&QualName::new(&q.name, "QSYS"), &l) {
                Ok(()) | Err(StoreError::Exists(_)) => {}
                Err(e) => return Err(e),
            }
        }
        lic.sync_definition(&p.definition);
    }
    store.save_licenses(&lic)
}

/// Parse a catalog in the shipped format.
pub fn parse(text: &str) -> Result<Catalog, Vec<pkg::ManifestError>> {
    let c: pkg::schema::Catalog = toml::from_str(text).map_err(|e| {
        vec![pkg::ManifestError {
            at: "catalog".into(),
            message: format!("not a valid catalog: {}", e.message()),
        }]
    })?;
    let opts = pkg::Options {
        allow_quartz: true,
        catalog: true,
    };
    let mut products = Vec::new();
    let mut errs = Vec::new();
    for (i, m) in c.package.iter().enumerate() {
        match pkg::build_manifest(m, opts) {
            Ok(mut p) => {
                if let Some(t) = agreement_text(p.definition.product.as_str()) {
                    p.agreement = Some(t.to_string());
                    let path = pkg::agreement_path(
                        &p.definition.product,
                        p.definition.release,
                        LoadId::ENGLISH,
                    );
                    for (_, l) in &mut p.loads {
                        if l.key.load_type == LoadType::Code && l.key.option == OptionNo::BASE {
                            l.agreements = vec![path.clone()];
                        }
                    }
                }
                products.push(p);
            }
            Err(e) => errs.extend(e.into_iter().map(|mut e| {
                e.at = format!("package[{i}].{}", e.at);
                e
            })),
        }
    }
    if errs.is_empty() {
        Ok(Catalog { products })
    } else {
        Err(errs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_with_its_products() {
        let c = shipped();
        let ids: Vec<&str> = c
            .products
            .iter()
            .map(|p| p.definition.product.as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "QS00SS1", "QS00WDS", "QS00TC1", "QS00ST1", "QS00DG1", "QS00NAV", "QS00AFW",
                "QS00PSE"
            ]
        );
        let reserved: Vec<&str> = c
            .reserved()
            .map(|p| p.definition.product.as_str())
            .collect();
        assert_eq!(reserved, ["QS00DG1", "QS00NAV", "QS00AFW", "QS00PSE"]);
        let os = c.product("5770SS1").unwrap();
        let opts: Vec<u8> = os.definition.options.iter().map(|o| o.option.0).collect();
        assert_eq!(opts, [0, 12, 18, 30, 33, 34]);
        assert_eq!(os.definition.release, release::RELEASE);
        let wds = c.product("QS00WDS").unwrap();
        assert!(wds.definition.option(OptionNo(31)).is_some());
        assert!(wds.definition.licensed);
        for p in &c.products {
            assert!(p.agreement.is_some(), "{}", p.definition.product);
            assert!(p.notes.iter().all(|(_, _, ibm)| ibm.is_some()));
        }
        // Every option has a code load and an English load.
        assert_eq!(os.loads.len(), 12);
        assert_eq!(os.loads[0].0.name, "SS100C5050");
        assert_eq!(os.loads[1].0.name, "SS100L2924");
    }
}
