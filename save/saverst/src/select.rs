// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Object selection: `OBJ`, `OBJTYPE`, `OMITOBJ`, generic names.

/// One selected or omitted name: `NAME`, `GEN*` or `*ALL`, with a type
/// or any type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjSel {
    /// The name, uppercase.
    pub name: String,
    /// The type code, `None` for `*ALL`.
    pub type_code: Option<u16>,
}

impl ObjSel {
    /// Every object.
    #[must_use]
    pub fn all() -> Self {
        Self {
            name: "*ALL".into(),
            type_code: None,
        }
    }

    /// Whether `name` of `type_code` is selected.
    #[must_use]
    pub fn matches(&self, name: &str, type_code: u16) -> bool {
        self.type_code.is_none_or(|t| t == type_code) && generic(&self.name, name)
    }
}

/// `*ALL`, an exact name, or a generic `ABC*`.
#[must_use]
pub fn generic(pattern: &str, name: &str) -> bool {
    if pattern == "*ALL" {
        return true;
    }
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

/// Whether an object passes a selection list (empty selects all) and
/// is not omitted.
#[must_use]
pub fn wanted(select: &[ObjSel], omit: &[ObjSel], name: &str, type_code: u16) -> bool {
    (select.is_empty() || select.iter().any(|s| s.matches(name, type_code)))
        && !omit.iter().any(|s| s.matches(name, type_code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_names() {
        assert!(generic("*ALL", "X"));
        assert!(generic("CUST*", "CUSTOMER"));
        assert!(!generic("CUST*", "ORDERS"));
        assert!(generic("A", "A"));
        assert!(!generic("A", "AB"));
        let omit = [ObjSel {
            name: "TMP*".into(),
            type_code: None,
        }];
        assert!(wanted(&[], &omit, "KEEP", 1));
        assert!(!wanted(&[], &omit, "TMP1", 1));
    }
}
