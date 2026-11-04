// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What `WRKHDWRSC`, `DSPHDWRSC` and the hardware APIs show of the
//! table: the resources of a `TYPE` in tree order, a resource's detail
//! and the descriptions of its place in the trees.

use std::fmt::Write as _;

use crate::table::{Kind, Resource, Status, Table};

/// The displays' words: the text of a `QSTXTMSG` identifier with its
/// values, in the job's language (the caller looks it up).
pub type Words<'a> = dyn FnMut(&str, &[&str]) -> String + 'a;

/// Words as their identifiers, for a caller that shows no text.
#[must_use]
pub fn ids(id: &str, _: &[&str]) -> String {
    id.to_string()
}

/// The `QSTXTMSG` identifier of a kind's short text.
#[must_use]
pub fn kind_id(k: Kind) -> &'static str {
    match k {
        Kind::Bridge => "TXT0140",
        Kind::Lan => "TXT0141",
        Kind::Storage => "TXT0142",
        Kind::Disk => "TXT0143",
        Kind::Optical => "TXT0144",
        Kind::Comm => "TXT0145",
        Kind::Port => "TXT0146",
        Kind::Keyboard => "TXT0147",
        Kind::Other => "TXT0148",
        Kind::Tape => "TXT0149",
        Kind::TapeLibrary => "TXT014A",
        Kind::Processor => "TXT014B",
        Kind::Memory => "TXT014C",
        Kind::Channel => "TXT014D",
    }
}

/// The `QSTXTMSG` identifier of a status.
#[must_use]
pub fn status_id(s: Status) -> &'static str {
    match s {
        Status::Operational => "TXT0150",
        Status::NotDetected => "TXT0151",
    }
}

/// How a type's list shows the type column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeCol {
    /// The type alone (`*CMN`, `*LWS`, `*CRP`).
    Type,
    /// Type and model (`*STG`, `*PRC`, `*CSA`).
    TypeModel,
}

/// One `TYPE` of the commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeSpec {
    /// The special value, `*CMN`.
    pub ty: &'static str,
    /// The word in the title (`Communication`): its `QSTXTMSG` identifier.
    pub word: &'static str,
    /// The panel of `QUIHDWRSC` that lists it.
    pub panel: &'static str,
    /// The kinds it lists.
    pub kinds: &'static [Kind],
    /// Whether the bridges above them show too.
    pub bridges: bool,
    /// The type column.
    pub col: TypeCol,
    /// `QGYRHRL`'s resource category.
    pub category: u32,
}

use Kind as K;

/// Every type, `*AHW` first.
pub const TYPES: &[TypeSpec] = &[
    TypeSpec {
        ty: "*AHW",
        word: "TXT0101",
        panel: "HWAHW",
        kinds: &[
            K::Processor,
            K::Memory,
            K::Bridge,
            K::Lan,
            K::Port,
            K::Comm,
            K::Channel,
            K::Storage,
            K::Disk,
            K::Optical,
            K::Tape,
            K::TapeLibrary,
            K::Keyboard,
            K::Other,
        ],
        bridges: true,
        col: TypeCol::TypeModel,
        category: 1,
    },
    TypeSpec {
        ty: "*CMN",
        word: "TXT0102",
        panel: "HWCMN",
        kinds: &[K::Lan, K::Port, K::Comm, K::Channel],
        bridges: true,
        col: TypeCol::Type,
        category: 2,
    },
    TypeSpec {
        ty: "*LWS",
        word: "TXT0103",
        panel: "HWLWS",
        kinds: &[K::Keyboard],
        bridges: true,
        col: TypeCol::Type,
        category: 3,
    },
    TypeSpec {
        ty: "*PRC",
        word: "TXT0104",
        panel: "HWPRC",
        kinds: &[K::Processor, K::Memory],
        bridges: false,
        col: TypeCol::TypeModel,
        category: 4,
    },
    TypeSpec {
        ty: "*STG",
        word: "TXT0105",
        panel: "HWSTG",
        kinds: &[K::Storage, K::Disk, K::Optical, K::Tape, K::TapeLibrary],
        bridges: true,
        col: TypeCol::TypeModel,
        category: 5,
    },
    TypeSpec {
        ty: "*CSA",
        word: "TXT0106",
        panel: "HWCSA",
        kinds: &[],
        bridges: false,
        col: TypeCol::TypeModel,
        category: 6,
    },
    TypeSpec {
        ty: "*CRP",
        word: "TXT0107",
        panel: "HWCRP",
        kinds: &[],
        bridges: false,
        col: TypeCol::Type,
        category: 8,
    },
];

/// The spec of a `TYPE`, `None` for a value the commands do not take.
#[must_use]
pub fn spec(ty: &str) -> Option<&'static TypeSpec> {
    let ty = ty.trim().to_ascii_uppercase();
    TYPES.iter().find(|s| s.ty == ty)
}

/// The spec of a `QGYRHRL` category: 1 all, 2 communication, 3 local
/// work station, 4 processor, 5 storage, 6 coupled, 8 cryptographic.
#[must_use]
pub fn by_category(n: u32) -> Option<&'static TypeSpec> {
    TYPES.iter().find(|s| s.category == n)
}

/// One row of a list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Resource name.
    pub name: String,
    /// Levels below the top of the list.
    pub depth: usize,
    /// The type column's text.
    pub type_col: String,
    /// `Operational`, `Not detected`.
    pub status: String,
    /// The short text.
    pub text: String,
}

impl Row {
    /// The name indented two blanks a level, as the lists show it.
    #[must_use]
    pub fn indented(&self) -> String {
        format!("{}{}", "  ".repeat(self.depth), self.name)
    }
}

/// The type and model as one value, `290A-001`; a disk, which has no
/// model, shows its type alone (`SSD`).
#[must_use]
pub fn type_model(r: &Resource) -> String {
    let (t, m) = r.type_model();
    if m.is_empty() { t } else { format!("{t}-{m}") }
}

/// The resources of a type in tree order (packaging tree).
#[must_use]
pub fn list(t: &Table, spec: &TypeSpec, w: &mut Words<'_>) -> Vec<Row> {
    t.tree(|r| spec.kinds.contains(&r.kind), false, spec.bridges)
        .into_iter()
        .map(|tr| {
            let r = &t.resources[tr.index];
            let (ty, _) = r.type_model();
            Row {
                name: r.name.clone(),
                depth: tr.depth,
                type_col: match spec.col {
                    // Four characters: `VIRT` for `VIRTUAL`.
                    TypeCol::Type => ty.chars().take(4).collect(),
                    // One blank before it, as the oracle's lists show.
                    TypeCol::TypeModel => format!(" {}", type_model(r)),
                },
                status: w(status_id(r.status), &[]),
                text: w(kind_id(r.kind), &[]),
            }
        })
        .collect()
}

/// A resource's detail: the values of the top of the display, its
/// location and the lines of the area below.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Detail {
    /// Resource name.
    pub name: String,
    /// Short text.
    pub text: String,
    /// `TTTT-MMM`.
    pub type_model: String,
    /// Serial number, blank when none.
    pub serial: String,
    /// Part number (none are known).
    pub part: String,
    /// Where it is: the PCI function and the unit on it.
    pub location: String,
    /// The lines below the location.
    pub lines: Vec<String>,
}

/// A label and value line under a heading: the label from column 4, the
/// value from column 37 (the area starts at column 2).
fn item(label: &str, value: &str) -> String {
    format!("  {label:<33}{value}")
}

/// A label and value line at the left edge, the value from column 37.
fn top(label: &str, value: &str) -> String {
    format!("{label:<35}{value}")
}

/// The location of a resource: its PCI function, then the port or unit
/// on it.
#[must_use]
pub fn location(r: &Resource) -> String {
    let pci = crate::scan::pci::address_in(&r.devpath);
    match (r.kind, pci) {
        (K::Port, Some(p)) => {
            let n = r.port.as_ref().map_or(0, |x| x.dev_port);
            format!("PCI {p}-T{n}")
        }
        (K::Disk | K::Optical | K::Tape | K::TapeLibrary, Some(p)) => {
            match crate::scan::storage::hctl(&r.devpath) {
                Some(a) => format!("PCI {p}-L{}:{}:{}:{}", a[0], a[1], a[2], a[3]),
                None => format!("PCI {p}-{}", r.location),
            }
        }
        (_, Some(p)) => format!("PCI {p}"),
        (_, None) => r.location.clone(),
    }
}

/// The detail of resource `name`, `None` when the table has none.
#[must_use]
pub fn detail(t: &Table, name: &str, w: &mut Words<'_>) -> Option<Detail> {
    let r = t.get(name)?;
    let mut lines = vec![String::new(), w("TXT0110", &[])];
    if let Some(p) = crate::scan::pci::address_in(&r.devpath) {
        // dddd:bb:dd.f
        let (dom, rest) = p.split_at(4);
        let bus = rest.get(1..3).unwrap_or("");
        let dev = rest.get(4..6).unwrap_or("");
        let func = rest.get(7..8).unwrap_or("");
        lines.push(format!(" {}", w("TXT0111", &[])));
        lines.push(item(&w("TXT0112", &[]), dom));
        lines.push(item(&w("TXT0113", &[]), bus));
        lines.push(item(&w("TXT0114", &[]), dev));
        lines.push(item(&w("TXT0115", &[]), func));
    }
    if let Some([h, c, tg, l]) = crate::scan::storage::hctl(&r.devpath) {
        lines.push(format!(" {}", w("TXT0116", &[])));
        lines.push(item(&w("TXT0117", &[]), &h.to_string()));
        lines.push(item(&w("TXT0118", &[]), &c.to_string()));
        lines.push(item(&w("TXT0119", &[]), &tg.to_string()));
        lines.push(item(&w("TXT011A", &[]), &l.to_string()));
    }
    lines.push(String::new());
    if let Some(p) = &r.port {
        lines.push(w("TXT011B", &[]));
        lines.push(item(&w("TXT011C", &[]), &p.ifname));
        lines.push(item(&w("TXT011D", &[]), &p.mac.replace(':', "").to_ascii_uppercase()));
        lines.push(item(&w("TXT011E", &[]), &p.driver));
        let link = match (p.carrier, p.speed) {
            (Some(true), Some(s)) => w("TXT011F", &[&s.to_string()]),
            (Some(true), None) => w("TXT0120", &[]),
            (Some(false), _) => w("TXT0121", &[]),
            (None, _) => w("TXT0122", &[]),
        };
        lines.push(item(&w("TXT0123", &[]), &link));
    }
    if !r.logical.is_empty() {
        lines.push(top(&w("TXT0124", &[]), &r.logical));
    }
    if !r.parent.is_empty() && r.parent != r.logical {
        lines.push(top(&w("TXT0125", &[]), &r.parent));
    }
    let attached: Vec<&Resource> = t
        .resources
        .iter()
        .filter(|u| u.logical == r.name && u.name != r.name)
        .collect();
    if !attached.is_empty() {
        lines.push(w("TXT0126", &[]));
        for u in attached {
            let mut v = format!("{} {}", w(kind_id(u.kind), &[]), w(status_id(u.status), &[]));
            if u.kind == K::Port
                && let Some(p) = &u.port
            {
                let _ = write!(v, " {}", p.ifname);
            }
            if matches!(u.kind, K::Disk | K::Optical) {
                let _ = write!(v, " {}", u.location);
            }
            lines.push(item(&u.name, &v));
        }
    }
    lines.push(top(&w("TXT0127", &[]), &r.description));
    Some(Detail {
        name: r.name.clone(),
        text: w(kind_id(r.kind), &[]),
        type_model: type_model(r),
        serial: r.serial.chars().take(20).collect(),
        part: String::new(),
        location: location(r),
        lines,
    })
}

/// The list as printed lines (`DSPHDWRSC OUTPUT(*PRINT)`, batch).
#[must_use]
pub fn print_lines(t: &Table, spec: &TypeSpec, w: &mut Words<'_>) -> Vec<String> {
    let word = w(spec.word, &[]);
    let heads = [w("TXT0129", &[]), w("TXT012A", &[]), w("TXT012B", &[]), w("TXT012C", &[])];
    let mut out = vec![
        w("TXT0128", &[&word]),
        String::new(),
        format!("{:<16}{:<12}{:<22}{}", heads[0], heads[1], heads[2], heads[3]),
    ];
    for row in list(t, spec, w) {
        let r = t.get(&row.name);
        let tm = r.map(type_model).unwrap_or_default();
        out.push(format!("{:<16}{:<12}{:<22}{}", row.indented(), tm, row.status, row.text));
    }
    if out.len() == 3 {
        let lower = word.to_lowercase();
        out.push(w("TXT012D", &[&lower]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::{Resource, Status};

    fn r(name: &str, kind: Kind, devpath: &str) -> Resource {
        Resource::new(name.into(), kind, String::new(), format!("{name} desc"), devpath.into())
    }

    fn sample() -> Table {
        let mut t = Table {
            resources: vec![
                r("CMB01", K::Bridge, "/devices/pci0000:00/0000:00:02.0"),
                r("LIN01", K::Lan, "/devices/pci0000:00/0000:00:02.0/0000:01:00.0"),
                r("CMN01", K::Port, "/devices/pci0000:00/0000:00:02.0/0000:01:00.0/virtio0/net/eth0"),
                r("DC01", K::Storage, "/devices/pci0000:00/0000:00:03.0"),
                r("DD001", K::Disk, "/devices/pci0000:00/0000:00:03.0/virtio1/block/vda"),
                r("CMB02", K::Bridge, "/devices/pci0000:00/0000:00:04.0"),
            ],
        };
        t.resources[1].type_model = Some(("QN01".into(), "001".into()));
        t.resources[2].type_model = Some(("QN01".into(), "001".into()));
        t.resources[4].status = Status::NotDetected;
        t.resources[4].location = "vda".into();
        t.link();
        t
    }

    #[test]
    fn communication_lists_the_bridge_adapter_and_port() {
        let t = sample();
        let rows = list(&t, spec("*cmn").unwrap(), &mut ids);
        let shown: Vec<(String, &str)> = rows.iter().map(|r| (r.indented(), r.type_col.as_str())).collect();
        assert_eq!(
            shown,
            [("CMB01".to_string(), "QB01"), ("  LIN01".into(), "QN01"), ("    CMN01".into(), "QN01")]
        );
        assert_eq!(rows[2].text, "TXT0146");
    }

    #[test]
    fn storage_shows_type_and_model_and_an_empty_bridge_is_left_out() {
        let t = sample();
        let rows = list(&t, spec("*STG").unwrap(), &mut ids);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].type_col, " QD00-000");
        assert_eq!(rows[1].indented(), "  DD001");
        assert_eq!(rows[1].status, "TXT0151");
        assert!(list(&t, spec("*CRP").unwrap(), &mut ids).is_empty());
        assert!(spec("*FOO").is_none());
        assert_eq!(by_category(5).unwrap().ty, "*STG");
    }

    #[test]
    fn a_controller_detail_names_its_units() {
        let t = sample();
        let d = detail(&t, "DC01", &mut ids).unwrap();
        assert_eq!(d.location, "PCI 0000:00:03.0");
        assert!(d.lines.iter().any(|l| l.contains("DD001") && l.contains("TXT0151 vda")), "{:?}", d.lines);
        let dd = detail(&t, "dd001", &mut ids).unwrap();
        assert!(dd.lines.iter().any(|l| l.trim_start().starts_with("TXT0124") && l.ends_with("DC01")));
        assert_eq!(&dd.lines[3][..35], format!("  {:<33}", "TXT0112"));
        assert!(detail(&t, "NONE", &mut ids).is_none());
        let p = print_lines(&t, spec("*CMN").unwrap(), &mut ids);
        assert!(p[4].starts_with("  LIN01         QN01-001"), "{p:?}");
    }
}
