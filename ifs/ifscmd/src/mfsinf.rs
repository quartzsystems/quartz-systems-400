// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPMFSINF` (`STATFS`): the attributes of the file system an object
//! is in, from `statvfs` and `pathconf` as the caller sees them, in the
//! primary's layout (`tools/conformance/golden/nfs/dspmfsinf-*`, the
//! listing `tools/conformance/oracle/nfs/print/`). The display and the
//! listing word three labels differently, as IBM's do.

use ifs::Errno;
use ifs::client::IfsClient;
use ifs::ops::StatVfs;
use ifs::ops::flags as f;

/// One row: the display's label, the listing's, and the value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The variable's name in the panel group.
    pub var: &'static str,
    /// The display's label.
    pub label: &'static str,
    /// The listing's label.
    pub print: &'static str,
    /// The value.
    pub value: String,
}

/// What `DSPMFSINF` shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// The object as given.
    pub object: String,
    /// The file system type, as IBM words it (`"root" (/)`, `/QSYS.LIB`).
    pub fs_type: String,
    /// The attributes, in order.
    pub rows: Vec<Row>,
}

/// The text of a limit: `No maximum` when there is none.
fn limit(v: Option<i64>) -> String {
    v.map_or_else(|| "No maximum".to_string(), |n| n.to_string())
}

fn yes(v: Option<i64>) -> String {
    if v.is_some_and(|n| n > 0) {
        "Yes"
    } else {
        "No"
    }
    .to_string()
}

/// The file system type's text for a mount's name.
#[must_use]
pub fn fs_type(name: &str) -> String {
    match name {
        // /dev is in the root file system on IBM i.
        "root" | "dev" => "\"root\" (/)".to_string(),
        // A remote mount (docs/NFS.md section 7): its server and path.
        // NEEDS-CODY: IBM's wording for an NFS mount (section 12).
        n if n.contains(':') => n.to_string(),
        n if n.starts_with('/') => n.to_string(),
        n => format!("/{n}"),
    }
}

/// The view of `path`'s file system; the IFS's error when it cannot be
/// reached.
pub fn view(c: &dyn IfsClient, path: &str) -> Result<View, Errno> {
    let v: StatVfs = c.statvfs(path)?;
    let pc = |name: i32| c.pathconf(path, name).ok().flatten();
    let blocks = |n: u64| {
        if v.blocks > 0 {
            n.to_string()
        } else if v.name == "QOPT" {
            "No maximum".to_string()
        } else {
            "Not defined".to_string()
        }
    };
    let row = |var, label, print, value: String| Row {
        var,
        label,
        print,
        value,
    };
    let links = limit(pc(f::PC_LINK_MAX));
    let rows = vec![
        row("BLKSIZE", "Block size", "Block size", v.bsize.to_string()),
        row("TOTBLK", "Total blocks", "Total blocks", blocks(v.blocks)),
        row("FREEBLK", "Blocks free", "Blocks free", blocks(v.bfree)),
        row(
            "OBJLNK",
            "Object link maximum",
            "Object link maximum",
            links.clone(),
        ),
        row(
            "DIRLNK",
            "Directory link maximum",
            "Directory link maximum",
            links,
        ),
        row(
            "NAMEMAX",
            "Pathname component maximum",
            "Path name component maximum",
            limit(pc(f::PC_NAME_MAX)),
        ),
        row(
            "PATHMAX",
            "Path name maximum",
            "Path name maximum",
            limit(pc(f::PC_PATH_MAX)),
        ),
        row(
            "CHOWN",
            "Change owner restricted",
            "Change owner restricted",
            yes(pc(f::PC_CHOWN_RESTRICTED)),
        ),
        row(
            "NOTRUNC",
            "No truncation",
            "No truncation",
            yes(pc(f::PC_NO_TRUNC)),
        ),
        row(
            "CASE",
            "Case Sensitivity",
            "Case Sensitivity",
            if v.case_sensitive { "Yes" } else { "No" }.to_string(),
        ),
    ];
    Ok(View {
        object: path.to_string(),
        fs_type: fs_type(&v.name),
        rows,
    })
}

/// A label with dot leaders to column 33 and the colon, as the display
/// and the listing write it.
fn lead(label: &str) -> String {
    let mut s = String::from(label);
    s.push(' ');
    // Dots on the even offsets from the label's start, through 30.
    while s.len() < 31 {
        s.push(if s.len() % 2 == 0 { ' ' } else { '.' });
    }
    format!("{s:<31}:   ")
}

/// The display's rows as text, for a job without a display.
#[must_use]
pub fn text(v: &View) -> String {
    use std::fmt::Write as _;
    let mut t = String::new();
    let _ = write!(t, " {}{}\n\n", lead("Object"), v.object);
    let _ = write!(t, " {}{}\n\n", lead("File system type"), v.fs_type);
    for r in &v.rows {
        let _ = writeln!(t, " {}{}", lead(r.label), r.value);
    }
    t
}

/// The listing's body (`OUTPUT(*PRINT)`): the object, the type and the
/// attributes, one a line, with the listing's labels.
#[must_use]
pub fn print_lines(v: &View) -> Vec<String> {
    let mut out = vec![
        format!(" {}{}", lead("Object"), v.object),
        format!(" {}{}", lead("File system type"), v.fs_type),
    ];
    out.extend(
        v.rows
            .iter()
            .map(|r| format!(" {}{}", lead(r.print), r.value)),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaders_as_ibms() {
        assert_eq!(lead("Object"), "Object . . . . . . . . . . . . :   ");
        assert_eq!(lead("Block size"), "Block size . . . . . . . . . . :   ");
        assert_eq!(lead("Blocks free"), "Blocks free  . . . . . . . . . :   ");
        assert_eq!(
            lead("Pathname component maximum"),
            "Pathname component maximum . . :   "
        );
        assert_eq!(
            lead("Path name component maximum"),
            "Path name component maximum  . :   "
        );
        assert_eq!(
            lead("File system type"),
            "File system type . . . . . . . :   "
        );
    }

    #[test]
    fn types() {
        assert_eq!(fs_type("root"), "\"root\" (/)");
        assert_eq!(fs_type("QSYS.LIB"), "/QSYS.LIB");
        assert_eq!(fs_type("QOpenSys"), "/QOpenSys");
        assert_eq!(fs_type("/dev/QASP01/a.udfs"), "/dev/QASP01/a.udfs");
        assert_eq!(fs_type("dev"), "\"root\" (/)");
    }
}
