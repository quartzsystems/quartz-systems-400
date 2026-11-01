// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The device parity controller. On IBM i parity lives in a SAS RAID
//! controller; QS/400 keeps it in the object store over plain disks, so
//! every system has one virtual controller that parity sets belong to.
//! It is named after the real controllers and is not in the table.

use super::{Kind, ParityCapable, Table};

/// The virtual controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParityController {
    /// Resource name: the storage controller number after the last real one.
    pub name: String,
    /// Type.
    pub kind: &'static str,
    /// Model.
    pub model: &'static str,
    /// What it is.
    pub text: &'static str,
}

impl ParityController {
    /// The line the service tools show.
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "{:<9} {}-{}  {}",
            self.name, self.kind, self.model, self.text
        )
    }
}

impl Table {
    /// The software device parity controller.
    #[must_use]
    pub fn software_parity(&self) -> ParityController {
        ParityController {
            name: self.next_name(Kind::Storage),
            kind: "QDPY",
            model: "001",
            text: "Software device parity controller",
        }
    }
}

impl ParityCapable for Table {
    fn parity_capable(&self) -> Vec<String> {
        vec![self.software_parity().line()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::Resource;

    #[test]
    fn named_after_the_real_controllers() {
        let mut t = Table::default();
        assert_eq!(t.software_parity().name, "DC01");
        t.resources.push(Resource::new(
            "DC01".into(),
            Kind::Storage,
            "0000:00:1f.2".into(),
            String::new(),
            String::new(),
        ));
        assert_eq!(t.software_parity().name, "DC02");
        assert_eq!(
            t.parity_capable(),
            vec!["DC02      QDPY-001  Software device parity controller".to_string()]
        );
    }
}
