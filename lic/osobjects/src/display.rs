// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped display files (`dspf::sys`), compiled from their DDS.

use qsname::Name;
use store::obj::{ObjectId, types};
use store::txn::Txn;

/// The extended attribute of a display file.
pub const ATTR: &str = "DSPF";

/// Compile DDS into a display file's payload: the payload, the
/// listing and the number of record formats, or the listing.
pub fn compile(
    src: &dspf::Source,
    refs: &dspf::MapRefs,
    name: &str,
) -> Result<(Vec<u8>, String, usize), String> {
    let comp = dspf::compile_dspf(
        src,
        refs,
        &dspf::CompileOpts {
            file: Some(name.to_string()),
            ..dspf::CompileOpts::default()
        },
    );
    let Some(df) = comp.definition else {
        return Err(format!("File {name} not created.\n{}", comp.listing));
    };
    let n = df.records.len();
    Ok((dspf::encode(&df), comp.listing, n))
}

/// The shipped display files missing from `QSYS`; how many were made.
pub fn install_shipped(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    stamp: &wm::obj::Stamp,
) -> Result<usize, String> {
    let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
    let mut made = 0;
    for (name, text) in dspf::sys::FILES {
        let n = Name::parse(name).map_err(|e| format!("{name}: {e}"))?;
        if lib
            .lookup(txn, types::FILE, n)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        let (src, _) = dspf::Source::from_text(text);
        let (payload, _, _) = compile(&src, &dspf::MapRefs::default(), name)?;
        wm::obj::create_attr(
            txn,
            qsys,
            (n, types::FILE, ATTR),
            &format!("Shipped display file {name}"),
            stamp,
            &payload,
        )
        .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPD0084")))?;
        made += 1;
    }
    Ok(made)
}
