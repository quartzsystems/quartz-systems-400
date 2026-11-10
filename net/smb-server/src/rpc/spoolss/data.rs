//! Printer data (MS-RPRN 3.1.4.2.7 RpcGetPrinterData, 3.1.4.2.19
//! RpcGetPrinterDataEx, 3.1.4.2.10 RpcEnumPrinterData, 3.1.4.2.20
//! RpcEnumPrinterDataEx, 3.1.4.2.21 RpcEnumPrinterKey): the server values
//! clients read to learn what the server is, a printer's ChangeID, and an
//! empty `PrinterDriverData` key. Any other value is ERROR_FILE_NOT_FOUND.

use super::info::Named;
use super::marshal::utf16z;
use super::session::{Session, Target};
use super::{Ctx, bad_stub, codes, dword, string_of, u32_of};
use crate::rpc::association::Fault;
use crate::rpc::ndr::{NdrReader, NdrWriter};

// Registry value types (MS-RPRN 2.2.3.9).
const REG_SZ: u32 = 1;
const REG_BINARY: u32 = 3;
const REG_DWORD: u32 = 4;

/// The key every printer has, with no values here.
const PRINTER_DRIVER_DATA: &str = "PrinterDriverData";

/// Largest data buffer a call may ask for.
const MAX_DATA: usize = 64 << 10;

/// One value: its type and bytes.
struct Value {
    name: &'static str,
    kind: u32,
    data: Vec<u8>,
}

fn dword_value(name: &'static str, value: u32) -> Value {
    Value {
        name,
        kind: REG_DWORD,
        data: value.to_le_bytes().to_vec(),
    }
}

/// OSVERSIONINFOW, or OSVERSIONINFOEXW with `ex`, for 10.0 (as srvsvc
/// reports), a server (MS-RPRN 2.2.3.10.1).
fn os_version(ex: bool) -> Vec<u8> {
    let size: u32 = if ex { 284 } else { 276 };
    let mut out = Vec::with_capacity(size as usize);
    for v in [size, 10, 0, 0, 2] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.resize(276, 0);
    if ex {
        // Service pack 0.0, no suite mask, VER_NT_SERVER, reserved.
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 3, 0]);
    }
    out
}

/// The values a handle's data holds.
async fn values(ctx: &Ctx<'_>, target: &Target) -> Option<Vec<Value>> {
    Some(match target {
        Target::Server => vec![
            dword_value("MajorVersion", 3),
            dword_value("MinorVersion", 0),
            Value {
                name: "Architecture",
                kind: REG_SZ,
                data: utf16z("Windows x64"),
            },
            dword_value("DsPresent", 0),
            dword_value("W3SvcInstalled", 0),
            Value {
                name: "DNSMachineName",
                kind: REG_SZ,
                data: utf16z(ctx.server),
            },
            Value {
                name: "OSVersion",
                kind: REG_BINARY,
                data: os_version(false),
            },
            Value {
                name: "OSVersionEx",
                kind: REG_BINARY,
                data: os_version(true),
            },
        ],
        Target::Printer(share) => {
            let printer = ctx.printer(share).await?;
            let named = Named {
                server: ctx.server,
                printer: &printer,
            };
            vec![dword_value("ChangeID", named.change_id())]
        }
    })
}

/// Whether `key` is one GetPrinterDataEx finds the values in: none (the
/// handle's own) or PrinterDriverData.
fn known_key(key: &str) -> bool {
    key.is_empty() || key.eq_ignore_ascii_case(PRINTER_DRIVER_DATA)
}

/// RpcGetPrinterData, and RpcGetPrinterDataEx (`ex`, with a key).
pub(crate) async fn get(
    ctx: &Ctx<'_>,
    session: &Session,
    r: &mut NdrReader<'_>,
    ex: bool,
) -> Result<Vec<u8>, Fault> {
    let handle = session.handle(r)?;
    let key = if ex { string_of(r)? } else { String::new() };
    let name = string_of(r)?;
    let size = u32_of(r)? as usize;
    if size > MAX_DATA || !r.at_end() {
        return Err(bad_stub());
    }
    let found = if known_key(&key) {
        values(ctx, &handle.target).await.and_then(|values| {
            values
                .into_iter()
                .find(|v| v.name.eq_ignore_ascii_case(&name))
        })
    } else {
        None
    };
    let mut buf = vec![0; size];
    let (kind, needed, status) = match found {
        None => (0, 0, codes::ERROR_FILE_NOT_FOUND),
        Some(v) if v.data.len() > size => (v.kind, v.data.len(), codes::ERROR_MORE_DATA),
        Some(v) => {
            buf[..v.data.len()].copy_from_slice(&v.data);
            (v.kind, v.data.len(), codes::ERROR_SUCCESS)
        }
    };
    let mut w = NdrWriter::new();
    w.u32(kind);
    w.bytes(&buf);
    w.u32(dword(needed));
    w.u32(status);
    Ok(w.finish())
}

/// RpcEnumPrinterData: the value at `dwIndex`, ERROR_NO_MORE_ITEMS past
/// the last.
pub(crate) async fn enum_values(
    ctx: &Ctx<'_>,
    session: &Session,
    r: &mut NdrReader<'_>,
) -> Result<Vec<u8>, Fault> {
    let handle = session.handle(r)?;
    let index = u32_of(r)? as usize;
    let name_size = u32_of(r)? as usize;
    let data_size = u32_of(r)? as usize;
    if name_size > MAX_DATA || data_size > MAX_DATA || !r.at_end() {
        return Err(bad_stub());
    }
    let values = values(ctx, &handle.target).await.unwrap_or_default();
    let mut name = vec![0; name_size / 2 * 2];
    let mut data = vec![0; data_size];
    let (name_needed, kind, data_needed, status) = match values.get(index) {
        None => (0, 0, 0, codes::ERROR_NO_MORE_ITEMS),
        Some(v) => {
            let units = utf16z(v.name);
            if units.len() > name.len() || v.data.len() > data.len() {
                (units.len(), v.kind, v.data.len(), codes::ERROR_MORE_DATA)
            } else {
                name[..units.len()].copy_from_slice(&units);
                data[..v.data.len()].copy_from_slice(&v.data);
                (units.len(), v.kind, v.data.len(), codes::ERROR_SUCCESS)
            }
        }
    };
    let mut w = NdrWriter::new();
    // [out, size_is(cbValueName/sizeof(wchar_t))] wchar_t*.
    w.u32(dword(name.len() / 2));
    w.raw(&name);
    w.u32(dword(name_needed));
    w.u32(kind);
    w.bytes(&data);
    w.u32(dword(data_needed));
    w.u32(status);
    Ok(w.finish())
}

/// RpcEnumPrinterDataEx: PrinterDriverData holds no values; other keys
/// are ERROR_FILE_NOT_FOUND.
pub(crate) async fn enum_values_ex(
    ctx: &Ctx<'_>,
    session: &Session,
    r: &mut NdrReader<'_>,
) -> Result<Vec<u8>, Fault> {
    let handle = session.handle(r)?;
    let key = string_of(r)?;
    let size = u32_of(r)? as usize;
    if size > MAX_DATA || !r.at_end() {
        return Err(bad_stub());
    }
    let exists = match &handle.target {
        Target::Server => false,
        Target::Printer(share) => ctx.printer(share).await.is_some(),
    };
    let status = if exists && key.eq_ignore_ascii_case(PRINTER_DRIVER_DATA) {
        codes::ERROR_SUCCESS
    } else {
        codes::ERROR_FILE_NOT_FOUND
    };
    // No PRINTER_ENUM_VALUES (MS-RPRN 2.2.2.11) to write.
    let mut w = NdrWriter::new();
    w.bytes(&vec![0; size]);
    w.u32(0);
    w.u32(0);
    w.u32(status);
    Ok(w.finish())
}

/// RpcEnumPrinterKey: a printer's top key holds PrinterDriverData, which
/// holds none; other keys are ERROR_FILE_NOT_FOUND.
pub(crate) fn enum_key(session: &Session, r: &mut NdrReader<'_>) -> Result<Vec<u8>, Fault> {
    let handle = session.handle(r)?;
    let key = string_of(r)?;
    let size = u32_of(r)? as usize;
    if size > MAX_DATA || !r.at_end() {
        return Err(bad_stub());
    }
    let subkeys: Option<Vec<u8>> = match (&handle.target, key.as_str()) {
        (Target::Printer(_), "") => {
            let mut list = utf16z(PRINTER_DRIVER_DATA);
            list.extend_from_slice(&[0, 0]);
            Some(list)
        }
        (Target::Printer(_), key) if key.eq_ignore_ascii_case(PRINTER_DRIVER_DATA) => {
            Some(vec![0, 0])
        }
        _ => None,
    };
    let mut buf = vec![0; size / 2 * 2];
    let (needed, status) = match subkeys {
        None => (0, codes::ERROR_FILE_NOT_FOUND),
        Some(list) if list.len() > buf.len() => (list.len(), codes::ERROR_MORE_DATA),
        Some(list) => {
            buf[..list.len()].copy_from_slice(&list);
            (list.len(), codes::ERROR_SUCCESS)
        }
    };
    let mut w = NdrWriter::new();
    w.u32(dword(buf.len() / 2));
    w.raw(&buf);
    w.u32(dword(needed));
    w.u32(status);
    Ok(w.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_versions_are_10_0_and_sized() {
        let v = os_version(false);
        assert_eq!(v.len(), 276);
        assert_eq!(&v[..8], &[0x14, 1, 0, 0, 10, 0, 0, 0]);
        let ex = os_version(true);
        assert_eq!(ex.len(), 284);
        assert_eq!(ex[282], 3, "VER_NT_SERVER");
    }

    #[test]
    fn keys_are_the_handles_own_or_printer_driver_data() {
        assert!(known_key(""));
        assert!(known_key("printerdriverdata"));
        assert!(!known_key("DsSpooler"));
    }
}
