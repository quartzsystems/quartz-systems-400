// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! File and print shares.

/// A file share's permission (`ZLSL0100`'s codes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    /// 1.
    ReadOnly,
    /// 2.
    ReadWrite,
}

impl Permission {
    /// The API code.
    #[must_use]
    pub fn code(self) -> i32 {
        match self {
            Self::ReadOnly => 1,
            Self::ReadWrite => 2,
        }
    }

    /// The permission of an API code.
    #[must_use]
    pub fn from_code(c: i32) -> Option<Self> {
        match c {
            1 => Some(Self::ReadOnly),
            2 => Some(Self::ReadWrite),
            _ => None,
        }
    }
}

/// A print share's spooled file type (`QZLSADPS`'s codes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpoolType {
    /// 1, `*USERASCII`.
    UserAscii,
    /// 2, `*AFP`.
    Afp,
    /// 3, `*SCS`.
    Scs,
    /// 4, `*AUTO`: sensed from the data.
    Auto,
}

impl SpoolType {
    /// The API code.
    #[must_use]
    pub fn code(self) -> i32 {
        match self {
            Self::UserAscii => 1,
            Self::Afp => 2,
            Self::Scs => 3,
            Self::Auto => 4,
        }
    }

    /// The type of an API code.
    #[must_use]
    pub fn from_code(c: i32) -> Option<Self> {
        match c {
            1 => Some(Self::UserAscii),
            2 => Some(Self::Afp),
            3 => Some(Self::Scs),
            4 => Some(Self::Auto),
            _ => None,
        }
    }

    /// `*USERASCII`, ... as `SERVER_SHARE_INFO` shows it.
    #[must_use]
    pub fn special(self) -> &'static str {
        match self {
            Self::UserAscii => "*USERASCII",
            Self::Afp => "*AFP",
            Self::Scs => "*SCS",
            Self::Auto => "*AUTO",
        }
    }
}

/// A library-qualified object name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Qualified {
    /// The object.
    pub name: String,
    /// Its library.
    pub library: String,
}

impl Qualified {
    /// `name` in `library`.
    #[must_use]
    pub fn new(name: &str, library: &str) -> Self {
        Self {
            name: name.into(),
            library: library.into(),
        }
    }

    /// Whether no object is named.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.name.is_empty()
    }
}

/// A file share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileShare {
    /// The IFS path shared.
    pub path: String,
    /// Read only or read/write.
    pub permission: Permission,
    /// Connections allowed at once; -1 no maximum.
    pub max_users: i32,
    /// CCSID text files are converted to; 0 the job's ASCII equivalent.
    pub text_ccsid: i32,
    /// Whether files with the extensions are converted as text.
    pub text_conversion: bool,
    /// The extensions converted (`TXT`, `MBR`, ...), up to 46 bytes each.
    pub extensions: Vec<String>,
}

/// A print share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrintShare {
    /// The output queue spooled files go to.
    pub outq: Qualified,
    /// The printer driver clients are told to use.
    pub driver: String,
    /// How data becomes a spooled file.
    pub spool_type: SpoolType,
    /// Printer file used to create the spooled files; empty for the
    /// job's default.
    pub printer_file: Qualified,
    /// Whether the share is published to the directory; always off
    /// (QS/400 has no directory server).
    pub publish: bool,
}

/// What a share shares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A directory.
    File(FileShare),
    /// An output queue.
    Print(PrintShare),
}

/// A share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Share {
    /// Up to 12 characters, upper case.
    pub name: String,
    /// Up to 50 characters.
    pub text: String,
    /// Whether clients must encrypt (SMB 3.x) to use it.
    pub encryption_required: bool,
    /// Authorization list whose `*USE` a user needs; empty for none.
    pub autl: String,
    /// File or print.
    pub kind: Kind,
    /// Records of the share file this release does not know, kept.
    pub unknown: Vec<(u16, Vec<u8>)>,
}

impl Share {
    /// A file share.
    #[must_use]
    pub fn file(name: &str, path: &str, text: &str) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
            encryption_required: false,
            autl: String::new(),
            kind: Kind::File(FileShare {
                path: path.into(),
                permission: Permission::ReadWrite,
                max_users: -1,
                text_ccsid: 0,
                text_conversion: false,
                extensions: Vec::new(),
            }),
            unknown: Vec::new(),
        }
    }

    /// A print share on `outq`.
    #[must_use]
    pub fn print(name: &str, outq: Qualified, text: &str) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
            encryption_required: false,
            autl: String::new(),
            kind: Kind::Print(PrintShare {
                outq,
                driver: String::new(),
                spool_type: SpoolType::Auto,
                printer_file: Qualified::default(),
                publish: false,
            }),
            unknown: Vec::new(),
        }
    }

    /// `ZLSL0100`'s device type: 0 file, 1 print.
    #[must_use]
    pub fn device_type(&self) -> i32 {
        match self.kind {
            Kind::File(_) => 0,
            Kind::Print(_) => 1,
        }
    }

    /// The file share's attributes.
    #[must_use]
    pub fn as_file(&self) -> Option<&FileShare> {
        match &self.kind {
            Kind::File(f) => Some(f),
            Kind::Print(_) => None,
        }
    }

    /// The print share's attributes.
    #[must_use]
    pub fn as_print(&self) -> Option<&PrintShare> {
        match &self.kind {
            Kind::Print(p) => Some(p),
            Kind::File(_) => None,
        }
    }
}

/// The share every system ships: `QIBM` on `/QIBM`.
#[must_use]
pub fn shipped() -> Vec<Share> {
    vec![Share::file("QIBM", "/QIBM", "QS/400 product and user data")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for c in 1..=2 {
            assert_eq!(Permission::from_code(c).map(Permission::code), Some(c));
        }
        for c in 1..=4 {
            assert_eq!(SpoolType::from_code(c).map(SpoolType::code), Some(c));
        }
        assert_eq!(Permission::from_code(0), None);
        assert_eq!(SpoolType::from_code(5), None);
    }

    #[test]
    fn the_shipped_share_is_qibm() {
        let s = shipped();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].name, "QIBM");
        assert_eq!(s[0].as_file().map(|f| f.path.as_str()), Some("/QIBM"));
        assert_eq!(s[0].device_type(), 0);
    }
}
