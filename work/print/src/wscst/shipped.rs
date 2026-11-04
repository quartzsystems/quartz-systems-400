// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The customizing objects shipped in `QSYS`: IBM's names (the
//! primary's `DSPOBJD QSYS/*ALL *WSCST`,
//! `tools/conformance/golden/phase12/oracle/wscst-qsys.txt`), Quartz
//! content. A LaserJet-class object holds the PCL transform's commands
//! for its model; a PCL-speaking printer of another maker the `*HP4`
//! ones; every other printer plain lines (CR LF, a form feed a page).
//! The three `QCTX...` objects are of class `CTXFORM` and hold nothing
//! the transform uses.

use crate::transform::Model;

/// The names, in `DSPOBJD`'s order.
pub const NAMES: &[&str] = &[
    "QCTXPDF", "QCTXPDFMBD", "QCTXPDFWT", "QWP4070EP", "QWPCPQPM20", "QWPDEFAULT", "QWPEAP2250",
    "QWPEAP3250", "QWPEAP5500", "QWPEFX1170", "QWPEFX5000", "QWPEFX850", "QWPELQ1070",
    "QWPELQ1170", "QWPELQ2550", "QWPELQ510", "QWPELQ860", "QWPELX810", "QWPEPLPCL4", "QWPEPLPCL5",
    "QWPESCP", "QWPESQ1170", "QWPGIF", "QWPHP550C", "QWPHP5SI", "QWPHPCOLOR", "QWPHPDBCS",
    "QWPHPDSKJT", "QWPHPEURO", "QWPHPLSR2", "QWPHPLSR2D", "QWPHPLSR3", "QWPHPLSR3D", "QWPHPLSR4",
    "QWPHPLSR5", "QWPHPLSR6", "QWPHPLSRE", "QWPHPXL300", "QWPIBM2380", "QWPIBM2390", "QWPIBM2412",
    "QWPIBM241B", "QWPIBM2705", "QWPIBM2706", "QWPIBM2770", "QWPIBM3130", "QWPIBM3812",
    "QWPIBM3816", "QWPIBM4019", "QWPIBM4029", "QWPIBM4070", "QWPIBM4072", "QWPIBM4202",
    "QWPIBM4208", "QWPIBM4212", "QWPIBM4226", "QWPIBM4232", "QWPIBM424D", "QWPIBM424L",
    "QWPIBM424M", "QWPIBM424X", "QWPIBM4308", "QWPIBM4312", "QWPIBM4320", "QWPIBM4321",
    "QWPIBM4324", "QWPIBM4332", "QWPIBM4770", "QWPIBM4912", "QWPIBM5152", "QWPIBM5201",
    "QWPIBM5202", "QWPIBM5204", "QWPIBM5216", "QWPIBM64EP", "QWPIBM64XX", "QWPIBM65EP",
    "QWPIBMESCP", "QWPIBMEURO", "QWPIBMIP2K", "QWPIOCANC", "QWPIP1116", "QWPIP1200", "QWPIP2190",
    "QWPIPCOLOR", "QWPLEX2380", "QWPLEX2390", "QWPLEX4227", "QWPLEXMRK", "QWPLEXMRKC", "QWPLEXOP",
    "QWPLEXOPC", "QWPLEXOPN", "QWPLEXOPS", "QWPLEXOPSC", "QWPLEXOPT", "QWPLIPS3", "QWPLIPS3NP",
    "QWPNEC201", "QWPNEC2200", "QWPNEC5300", "QWPNEC6300", "QWPNECP2", "QWPOKI184", "QWPOKI320",
    "QWPOKI3410", "QWPOKI390", "QWPOKI393", "QWPOKI400", "QWPOKI590", "QWPOKI800", "QWPOKI810",
    "QWPPAGE300", "QWPPAGE3NP", "QWPPAGES", "QWPPAGESNP", "QWPPAN1123", "QWPPAN1124", "QWPPAN1180",
    "QWPPAN1695", "QWPPAN2123", "QWPPAN2124", "QWPPAN2180", "QWPPAN2624", "QWPPANDP", "QWPPANDP1",
    "QWPPANDPC", "QWPPANLBP4", "QWPPANLBP5", "QWPRICOH1", "QWPRICOH10", "QWPRICOH11", "QWPRICOH12",
    "QWPRICOH13", "QWPRICOH14", "QWPRICOH15", "QWPRICOH16", "QWPRICOH17", "QWPRICOH18",
    "QWPRICOH2", "QWPRICOH3", "QWPRICOH4", "QWPRICOH5", "QWPRICOH6", "QWPRICOH7", "QWPRICOH8",
    "QWPRICOH9", "QWPTIFFG4", "QWPTIFFPB", "QWPXRX4219", "QWPXRX4220", "QWPXRX4235", "QWPXRX4700",
];

/// The object `MFRTYPMDL` names for a LaserJet-class model.
#[must_use]
pub fn of_model(m: Model) -> &'static str {
    match m {
        Model::HpII | Model::HpIIP | Model::HpPlus => "QWPHPLSR2",
        Model::HpIID => "QWPHPLSR2D",
        Model::HpIII | Model::HpIIIP => "QWPHPLSR3",
        Model::HpIIID | Model::HpIIISI => "QWPHPLSR3D",
        Model::Hp4 => "QWPHPLSR4",
        Model::Hp5 => "QWPHPLSR5",
        Model::Hp5SI => "QWPHP5SI",
    }
}

/// The shipped object an `MFRTYPMDL` value names: a LaserJet-class
/// model's, else `QWP` and the value without its `*` when that is one
/// (`*IBM4019` is `QWPIBM4019`); `None` for `*WSCST`, `*NONE`, `*PDF`.
#[must_use]
pub fn for_mfrtypmdl(value: &str) -> Option<&'static str> {
    let up = value.trim().to_ascii_uppercase();
    if let Some(m) = Model::parse(&up) {
        return Some(of_model(m));
    }
    let rest = up.strip_prefix('*')?;
    let name = format!("QWP{rest}");
    NAMES.iter().copied().find(|n| *n == name)
}

/// The model whose commands an object holds; `None` for plain lines.
fn model_of(name: &str) -> Option<Model> {
    if let Some(m) = Model::all().into_iter().find(|m| of_model(*m) == name) {
        return Some(m);
    }
    let pcl = ["QWPHP", "QWPLEX", "QWPEPLPCL", "QWPCPQPM20", "QWPPANLBP"];
    pcl.iter().any(|p| name.starts_with(p)).then_some(Model::Hp4)
}

/// The class (`DEVCLASS`, the object's attribute) of a shipped object.
#[must_use]
pub fn class(name: &str) -> &'static str {
    if name.starts_with("QCTX") { "CTXFORM" } else { "TRANSFORM" }
}

/// The source of a shipped object.
#[must_use]
pub fn source(name: &str) -> String {
    if class(name) == "CTXFORM" {
        return format!("/* {name}: a content transform object; the host print transform uses none of it. */\n:WSCST DEVCLASS=CTXFORM.\n:EWSCST.\n");
    }
    match model_of(name) {
        Some(m) => super::Wscst::source_for(m).replacen(
            &format!("/* Workstation customizing source for {}", m.name()),
            &format!("/* {name}: workstation customizing source for {}", m.name()),
            1,
        ),
        None => format!(
            "/* {name}: plain lines, as the transform sends a printer it has no commands for. */\n\
             :WSCST DEVCLASS=TRANSFORM.\n\n    :TRNSFRMTBL.\n    :CARRTN\n      DATA='0D'X.\n    :LINEFEED\n      DATA='0A'X.\n    :FORMFEED\n      DATA='0C'X.\n:EWSCST.\n"
        ),
    }
}

/// Every shipped object: name, class, compiled payload.
#[must_use]
pub fn objects() -> Vec<(&'static str, &'static str, Vec<u8>)> {
    NAMES
        .iter()
        .map(|n| {
            let w = super::Wscst::parse(&source(n)).unwrap_or_default();
            (*n, class(n), w.encode())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_object_compiles() {
        assert_eq!(NAMES.len(), 153);
        for n in NAMES {
            let w = super::super::Wscst::parse(&source(n)).unwrap_or_else(|e| panic!("{n}: {e}"));
            assert_eq!(super::super::Wscst::decode(&w.encode()).unwrap().init, w.init, "{n}");
        }
        for m in Model::all() {
            assert!(NAMES.contains(&of_model(m)), "{}", m.name());
        }
        let hp4 = super::super::Wscst::parse(&source("QWPHPLSR4")).unwrap();
        assert!(hp4.init.starts_with(b"\x1bE"));
        assert_eq!(hp4.duplex.len(), 3);
        let plain = super::super::Wscst::parse(&source("QWPIBM4019")).unwrap();
        assert!(plain.init.is_empty() && plain.newline == b"\r\n");
    }
}
