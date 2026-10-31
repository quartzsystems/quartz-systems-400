// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The hardware resource APIs over the hardware resource service's
//! table (`lic/hwrsrc`): `QGYRHRL` lists the resources of a category
//! in format `RHRL0100`, `QGYRHRI` describes one in `RHRI0100`. The
//! layouts are in `docs/SYSAPI.md`, "Hardware resources".

use hwrsrc::view;
use hwrsrc::{Resource, Table};
use mi::Job;

use crate::param::{Args, ErrCode, Out};
use crate::provider::{ApiFail, Provider};

/// The APIs.
pub const APIS: &[(&str, crate::ApiFn)] = &[("QGYRHRL", qgyrhrl), ("QGYRHRI", qgyrhri)];

/// Length of an `RHRL0100` entry.
pub const RHRL_ENTRY: usize = 100;

/// The status byte: `1` operational, `2` not detected.
fn status(r: &Resource) -> char {
    if r.operational() { '1' } else { '2' }
}

/// The fields every format starts a resource with, at `off`: category,
/// family level, line type, name, type, model, status, the system an
/// adapter connects (blank), the adapter address, the description.
fn common(o: &mut Out, off: usize, category: u32, depth: usize, r: &Resource) {
    let (ty, md) = r.type_model();
    o.bin4(off, i32::try_from(category).unwrap_or(1));
    o.bin4(off + 4, i32::try_from(depth + 1).unwrap_or(1));
    o.bin4(off + 8, 0);
    o.char(off + 12, 10, &r.name);
    o.char(off + 22, 4, &ty);
    o.char(off + 26, 3, &md);
    o.ch(off + 29, status(r));
    o.char(off + 30, 8, "");
    let mac = r.port.as_ref().map(|p| p.mac.replace(':', "").to_ascii_uppercase()).unwrap_or_default();
    o.char(off + 38, 12, &mac);
    o.char(off + 50, 50, r.kind.text());
}

/// `RHRL0100` of category `cat`: bytes returned and available, the
/// count, the entry length, then one 100-byte entry per resource in
/// the list's tree order. `None` for a category the API does not know.
#[must_use]
pub fn list_image(t: &Table, cat: u32) -> Option<Out> {
    let spec = view::by_category(cat)?;
    let rows = view::list(t, spec);
    let mut o = Out::new();
    o.zero_to(16);
    o.bin4(8, i32::try_from(rows.len()).unwrap_or(0));
    o.bin4(12, i32::try_from(RHRL_ENTRY).unwrap_or(0));
    for (i, row) in rows.iter().enumerate() {
        if let Some(r) = t.get(&row.name) {
            common(&mut o, 16 + i * RHRL_ENTRY, cat, row.depth, r);
        }
    }
    Some(o)
}

/// `RHRI0100` of resource `name`: the common fields from offset 8, then
/// the serial number (10), part number (12), the packaging and logical
/// parents (10 each) and the location (79): 229 bytes.
#[must_use]
pub fn info_image(t: &Table, name: &str) -> Option<Out> {
    let r = t.get(name.trim())?;
    let cat = view::TYPES
        .iter()
        .skip(1)
        .find(|s| s.kinds.contains(&r.kind))
        .map_or(1, |s| s.category);
    let depth = t.path_to(&r.name, false).len().saturating_sub(1);
    let mut o = Out::new();
    o.zero_to(8);
    common(&mut o, 8, cat, depth, r);
    o.char(108, 10, &r.serial.chars().take(10).collect::<String>());
    o.char(118, 12, "");
    o.char(130, 10, &r.parent);
    o.char(140, 10, &r.logical);
    o.char(150, 79, &view::location(r));
    Some(o)
}

/// The table from the provider, or the error code's message.
fn table(j: &mut Job, p: &mut dyn Provider, e: ErrCode) -> Option<Table> {
    match p.hardware() {
        Ok(t) => Some(t),
        Err(f) => {
            e.fail(j, &f);
            None
        }
    }
}

/// `QGYRHRL(receiver, length, format, resource category, errc)`.
fn qgyrhrl(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let e = ErrCode::at(&a, 4);
    let fmt = a.char(2, 8);
    if fmt != "RHRL0100" {
        e.fail(j, &ApiFail::id("CPF3C21").text(&fmt, 8));
        return;
    }
    let Some(t) = table(j, p, e) else { return };
    let cat = u32::try_from(a.bin4(3)).unwrap_or(0);
    match list_image(&t, cat) {
        Some(o) => {
            o.deliver(&a, 0, 1);
            e.ok();
        }
        None => e.fail(j, &ApiFail::id("CPF9898").text(&format!("Resource category {cat} is not valid."), 512)),
    }
}

/// `QGYRHRI(receiver, length, format, resource name, errc)`.
fn qgyrhri(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let e = ErrCode::at(&a, 4);
    let fmt = a.char(2, 8);
    if fmt != "RHRI0100" {
        e.fail(j, &ApiFail::id("CPF3C21").text(&fmt, 8));
        return;
    }
    let name = a.char(3, 10);
    let Some(t) = table(j, p, e) else { return };
    match info_image(&t, &name) {
        Some(o) => {
            o.deliver(&a, 0, 1);
            e.ok();
        }
        None => e.fail(j, &ApiFail::id("CPF9898").text(&format!("Hardware resource {} was not found.", name.trim()), 512)),
    }
}

#[cfg(test)]
mod tests {
    use hwrsrc::{Kind, Resource, Table};

    use super::*;

    fn table() -> Table {
        let r = |n: &str, k: Kind, p: &str| Resource::new(n.into(), k, String::new(), String::new(), p.into());
        let mut t = Table {
            resources: vec![
                r("DC01", Kind::Storage, "/devices/pci0000:00/0000:00:03.0"),
                r("DD001", Kind::Disk, "/devices/pci0000:00/0000:00:03.0/virtio1/block/vda"),
                r("CMN01", Kind::Port, "/devices/pci0000:00/0000:00:02.0/virtio0/net/eth0"),
            ],
        };
        t.resources[1].serial = "QS400DISK0001".into();
        t.link();
        t
    }

    fn text(b: &[u8]) -> String {
        ccsid::decode_trimmed(b)
    }

    #[test]
    fn the_storage_list_and_one_unit() {
        let t = table();
        let o = list_image(&t, 5).unwrap();
        let b = &o.buf;
        assert_eq!(crate::param::bin4_at(b, 8), 2);
        assert_eq!(crate::param::bin4_at(b, 12), 100);
        assert_eq!(b.len(), 16 + 200);
        assert_eq!(text(&b[16 + 12..16 + 22]), "DC01");
        assert_eq!(text(&b[116 + 12..116 + 22]), "DD001");
        assert_eq!(crate::param::bin4_at(b, 116 + 4), 2);
        assert_eq!(text(&b[116 + 22..116 + 29]), "QU00001");
        assert!(list_image(&t, 99).is_none());

        let o = info_image(&t, "DD001   ").unwrap();
        let b = &o.buf;
        assert_eq!(b.len(), 229);
        assert_eq!(text(&b[20..30]), "DD001");
        assert_eq!(text(&b[108..118]), "QS400DISK0");
        assert_eq!(text(&b[140..150]), "DC01");
        assert!(text(&b[150..229]).starts_with("PCI 0000:00:03.0"));
        assert!(info_image(&t, "NONE").is_none());
    }
}
