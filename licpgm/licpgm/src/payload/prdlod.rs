// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*PRDLOD` payload (`docs/LICPGM.md` "Product load").

use crate::id::{self, LoadId, LoadType, OptionNo};
use crate::model::QualName;
use crate::model::definition::{RegType, Registration};
use crate::model::load::{
    DirEntry, FolderEntry, LibEntry, LibType, LoadKey, ObjEntry, ProductLoad,
};
use crate::model::state::LoadState;
use crate::payload::PayloadError;
use crate::payload::codec::{Reader, Writer, get_text, put_text};
use crate::payload::spec::{Layout, field, var};

/// Magic of the payload.
pub const MAGIC: &[u8; 8] = b"QSPRDLOD";
/// Format version.
pub const VERSION: u16 = 1;

/// The fixed header.
pub static HEADER: Layout = Layout {
    name: "PrdLodHeader",
    size: Some(160),
    fields: &[
        field("magic", 0, 8),
        field("format_version", 8, 2),
        field("flags", 10, 2),
        field("product_id", 12, 7),
        field("release", 19, 6),
        field("option", 25, 2),
        field("load_type", 27, 1),
        field("load_id", 28, 4),
        field("state", 32, 2),
        field("registration_type", 34, 2),
        field("registration_value", 36, 14),
        field("min_target", 50, 6),
        field("secondary_lib", 56, 10),
        field("text", 66, 50),
        field("public_aut", 116, 10),
        field("library_count", 126, 2),
        field("installed_at", 128, 8),
        field("pre_exit_count", 136, 2),
        field("folder_count", 138, 2),
        field("directory_count", 140, 2),
        field("agreement_count", 142, 2),
        field("object_count", 144, 4),
        field("path_count", 148, 4),
        field("reserved", 152, 8),
    ],
};

/// One library.
pub static LIBRARY: Layout = Layout {
    name: "PrdLodLibrary",
    size: Some(72),
    fields: &[
        field("dev", 0, 10),
        field("primary", 10, 10),
        field("installed", 20, 10),
        field("post_exit", 30, 10),
        field("lib_type", 40, 10),
        field("aut", 50, 10),
        field("crt_aut", 60, 10),
        field("reserved", 70, 2),
    ],
};

/// One preoperation exit program.
pub static EXIT: Layout = Layout {
    name: "PrdLodExit",
    size: Some(20),
    fields: &[field("program", 0, 10), field("library", 10, 10)],
};

/// One folder.
pub static FOLDER: Layout = Layout {
    name: "PrdLodFolder",
    size: Some(192),
    fields: &[
        field("dev", 0, 63),
        field("primary", 63, 63),
        field("installed", 126, 63),
        field("reserved", 189, 3),
    ],
};

/// One directory; its names follow the fixed part.
pub static DIRECTORY: Layout = Layout {
    name: "PrdLodDirectory",
    size: None,
    fields: &[
        field("primary_len", 0, 2),
        field("home_len", 2, 2),
        field("installed_len", 4, 2),
        field("installed_home_len", 6, 2),
        field("data_aut", 8, 10),
        field("obj_aut_count", 18, 2),
        crate::payload::spec::Field {
            name: "primary",
            offset: Some(20),
            size: None,
        },
        var("home"),
        var("installed"),
        var("installed_home"),
        var("obj_auts"),
    ],
};

/// One object.
pub static OBJECT: Layout = Layout {
    name: "PrdLodObject",
    size: Some(32),
    fields: &[
        field("library", 0, 10),
        field("name", 10, 10),
        field("type", 20, 10),
        field("reserved", 30, 2),
    ],
};

/// A length-prefixed UTF-8 text: a path or an agreement document.
pub static TEXT: Layout = Layout {
    name: "PrdLodText",
    size: None,
    fields: &[
        field("len", 0, 2),
        crate::payload::spec::Field {
            name: "text",
            offset: Some(2),
            size: None,
        },
    ],
};

const F_SUPPORTED: u16 = 1;
const F_OWN_REG: u16 = 2;

/// Encode a load.
pub fn encode(l: &ProductLoad) -> Result<Vec<u8>, PayloadError> {
    let mut w = Writer::default();
    w.bytes(MAGIC);
    w.u16(VERSION);
    let mut flags = 0;
    if l.supported {
        flags |= F_SUPPORTED;
    }
    if l.registration.is_some() {
        flags |= F_OWN_REG;
    }
    w.u16(flags);
    w.char(l.key.product.as_str(), 7, "product_id")?;
    w.char(&id::release_text(l.key.release), 6, "release")?;
    w.u16(u16::from(l.key.option.0));
    w.u8(match l.key.load_type {
        LoadType::Code => 0,
        LoadType::Lng => 1,
    });
    w.char(l.key.load_id.as_str(), 4, "load_id")?;
    w.char(l.state.code(), 2, "state")?;
    let (rt, rv) = l
        .registration
        .as_ref()
        .map_or(("", ""), |r| (r.kind.code(), r.value.as_str()));
    w.char(rt, 2, "registration_type")?;
    w.char(rv, 14, "registration_value")?;
    w.char(
        &l.min_target.map(id::release_text).unwrap_or_default(),
        6,
        "min_target",
    )?;
    w.char(
        l.secondary_lib.as_deref().unwrap_or(""),
        10,
        "secondary_lib",
    )?;
    w.char(&l.text, 50, "text")?;
    w.char(&l.public_aut, 10, "public_aut")?;
    w.count16(l.libraries.len(), "library_count")?;
    w.u64(l.installed_at);
    w.count16(l.pre_exits.len(), "pre_exit_count")?;
    w.count16(l.folders.len(), "folder_count")?;
    w.count16(l.directories.len(), "directory_count")?;
    w.count16(l.agreements.len(), "agreement_count")?;
    w.u32(u32::try_from(l.objects.len()).map_err(|_| PayloadError::TooMany("objects"))?);
    w.u32(u32::try_from(l.paths.len()).map_err(|_| PayloadError::TooMany("paths"))?);
    w.zeros(8);
    debug_assert_eq!(Some(w.buf.len()), HEADER.size);
    for lib in &l.libraries {
        w.char(&lib.dev, 10, "dev")?;
        w.char(&lib.primary, 10, "primary")?;
        w.char(&lib.installed, 10, "installed")?;
        w.char(lib.post_exit.as_deref().unwrap_or(""), 10, "post_exit")?;
        w.char(lib.lib_type.name(), 10, "lib_type")?;
        w.char(&lib.aut, 10, "aut")?;
        w.char(&lib.crt_aut, 10, "crt_aut")?;
        w.zeros(2);
    }
    for e in &l.pre_exits {
        w.char(&e.name, 10, "program")?;
        w.char(&e.lib, 10, "library")?;
    }
    for f in &l.folders {
        w.char(&f.dev, 63, "folder")?;
        w.char(&f.primary, 63, "folder")?;
        w.char(&f.installed, 63, "folder")?;
        w.zeros(3);
    }
    for d in &l.directories {
        for s in [&d.primary, &d.home, &d.installed, &d.installed_home] {
            w.count16(s.len(), "directory")?;
        }
        w.char(&d.data_aut, 10, "data_aut")?;
        w.count16(d.obj_auts.len(), "obj_aut_count")?;
        for s in [&d.primary, &d.home, &d.installed, &d.installed_home] {
            w.utf8(s);
        }
        for a in &d.obj_auts {
            w.char(a, 10, "obj_auts")?;
        }
    }
    for o in &l.objects {
        w.char(&o.lib, 10, "library")?;
        w.char(&o.name, 10, "name")?;
        w.char(&o.typ, 10, "type")?;
        w.zeros(2);
    }
    for p in &l.paths {
        put_text(&mut w, p, "path")?;
    }
    for a in &l.agreements {
        put_text(&mut w, a, "agreement")?;
    }
    Ok(w.buf)
}

/// Decode a load.
pub fn decode(b: &[u8]) -> Result<ProductLoad, PayloadError> {
    let mut r = Reader::new(b);
    if r.take(8)? != MAGIC {
        return Err(PayloadError::Magic);
    }
    let v = r.u16()?;
    if v != VERSION {
        return Err(PayloadError::Version(v));
    }
    let flags = r.u16()?;
    let product = r
        .char(7)?
        .parse()
        .map_err(|_| PayloadError::Bad("product_id"))?;
    let release = id::parse_release(&r.char(6)?).map_err(|_| PayloadError::Bad("release"))?;
    let option = u8::try_from(r.u16()?)
        .ok()
        .filter(|n| *n <= 99)
        .map(OptionNo)
        .ok_or(PayloadError::Bad("option"))?;
    let load_type = match r.u8()? {
        0 => LoadType::Code,
        1 => LoadType::Lng,
        _ => return Err(PayloadError::Bad("load_type")),
    };
    let load_id =
        LoadId::for_type(load_type, &r.char(4)?).map_err(|_| PayloadError::Bad("load_id"))?;
    let state = LoadState::from_code(&r.char(2)?).ok_or(PayloadError::Bad("state"))?;
    let rt = r.char(2)?;
    let rv = r.char(14)?;
    let registration = if flags & F_OWN_REG != 0 {
        Some(Registration {
            kind: RegType::from_code(&rt).ok_or(PayloadError::Bad("registration_type"))?,
            value: rv,
        })
    } else {
        None
    };
    let mt = r.char(6)?;
    let min_target = if mt.is_empty() {
        None
    } else {
        Some(id::parse_release(&mt).map_err(|_| PayloadError::Bad("min_target"))?)
    };
    let sl = r.char(10)?;
    let text = r.char(50)?;
    let public_aut = r.char(10)?;
    let n_lib = r.u16()?;
    let installed_at = r.u64()?;
    let n_exit = r.u16()?;
    let n_fld = r.u16()?;
    let n_dir = r.u16()?;
    let n_agr = r.u16()?;
    let n_obj = r.u32()?;
    let n_path = r.u32()?;
    r.skip(8)?;
    let mut libraries = Vec::new();
    for _ in 0..n_lib {
        let dev = r.char(10)?;
        let primary = r.char(10)?;
        let installed = r.char(10)?;
        let pe = r.char(10)?;
        let lib_type = match r.char(10)?.as_str() {
            "*PROD" => LibType::Prod,
            "*TEST" => LibType::Test,
            _ => return Err(PayloadError::Bad("lib_type")),
        };
        let aut = r.char(10)?;
        let crt_aut = r.char(10)?;
        r.skip(2)?;
        libraries.push(LibEntry {
            dev,
            primary,
            installed,
            post_exit: (!pe.is_empty()).then_some(pe),
            lib_type,
            aut,
            crt_aut,
        });
    }
    let mut pre_exits = Vec::new();
    for _ in 0..n_exit {
        let n = r.char(10)?;
        let l = r.char(10)?;
        pre_exits.push(QualName::new(&n, &l));
    }
    let mut folders = Vec::new();
    for _ in 0..n_fld {
        let dev = r.char(63)?;
        let primary = r.char(63)?;
        let installed = r.char(63)?;
        r.skip(3)?;
        folders.push(FolderEntry {
            dev,
            primary,
            installed,
        });
    }
    let mut directories = Vec::new();
    for _ in 0..n_dir {
        let lens = [r.u16()?, r.u16()?, r.u16()?, r.u16()?];
        let data_aut = r.char(10)?;
        let n_aut = r.u16()?;
        let mut s = Vec::new();
        for n in lens {
            s.push(r.utf8(usize::from(n), "directory")?);
        }
        let mut obj_auts = Vec::new();
        for _ in 0..n_aut {
            obj_auts.push(r.char(10)?);
        }
        let [primary, home, installed, installed_home]: [String; 4] =
            s.try_into().expect("four names");
        directories.push(DirEntry {
            primary,
            home,
            installed,
            installed_home,
            data_aut,
            obj_auts,
        });
    }
    let mut objects = Vec::new();
    for _ in 0..n_obj {
        let lib = r.char(10)?;
        let name = r.char(10)?;
        let typ = r.char(10)?;
        r.skip(2)?;
        objects.push(ObjEntry { lib, name, typ });
    }
    let mut paths = Vec::new();
    for _ in 0..n_path {
        paths.push(get_text(&mut r, "path")?);
    }
    let mut agreements = Vec::new();
    for _ in 0..n_agr {
        agreements.push(get_text(&mut r, "agreement")?);
    }
    if r.pos() != b.len() {
        return Err(PayloadError::Trailing(b.len() - r.pos()));
    }
    Ok(ProductLoad {
        key: LoadKey {
            product,
            release,
            option,
            load_type,
            load_id,
        },
        registration,
        min_target,
        secondary_lib: (!sl.is_empty()).then_some(sl),
        libraries,
        pre_exits,
        folders,
        directories,
        objects,
        paths,
        agreements,
        text,
        public_aut,
        state,
        supported: flags & F_SUPPORTED != 0,
        installed_at,
    })
}
