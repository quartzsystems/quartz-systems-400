//! PRINTER_INFO, DRIVER_INFO and JOB_INFO as custom-marshaled blocks
//! (MS-RPRN 2.2.2.4, 2.2.2.6, 2.2.2.9), the DEVMODE a printer starts
//! documents with (2.2.2.1) and its security descriptor ([MS-DTYP] 2.4.6).

use std::time::{SystemTime, UNIX_EPOCH};

use super::forms;
use super::marshal::{Block, utf16z};
use crate::print::{DriverFiles, JobInfo, PrinterInfo};

/// PRINTER_ATTRIBUTE_SHARED | PRINTER_ATTRIBUTE_LOCAL |
/// PRINTER_ATTRIBUTE_RAW_ONLY (MS-RPRN 2.2.3.12): the client renders, and
/// sends the printer's own language.
pub(crate) const PRINTER_ATTRIBUTES: u32 = 0x0000_0008 | 0x0000_0040 | 0x0000_1000;

/// PRINTER_ENUM_ICON8, the flag of a printer in PRINTER_INFO_1.
const PRINTER_ENUM_ICON8: u32 = 0x0080_0000;

/// DSPRINT_UNPUBLISH: the printer is not in a directory (PRINTER_INFO_7).
const DSPRINT_UNPUBLISH: u32 = 0x0000_0004;

/// The driver version told when the share has no files (MS-RPRN
/// 2.2.1.3.1: user-mode drivers).
pub(crate) const DRIVER_VERSION_3: u32 = 3;

/// The only data type taken (MS-RPRN 2.2.1.8, RAW).
pub(crate) const RAW: &str = "RAW";

/// The print processor named (Windows' own name for RAW handling).
const PRINT_PROCESSOR: &str = "winprint";

/// A printer as the info levels name it.
pub(crate) struct Named<'a> {
    pub server: &'a str,
    pub printer: &'a PrinterInfo,
}

impl Named<'_> {
    fn server_unc(&self) -> String {
        format!("\\\\{}", self.server)
    }

    fn printer_unc(&self) -> String {
        format!("\\\\{}\\{}", self.server, self.printer.share)
    }

    /// cChangeID: a value that changes when what the levels report does.
    pub(crate) fn change_id(&self) -> u32 {
        let p = self.printer;
        let mut hash: u32 = 0x811C_9DC5;
        for part in [
            &p.share,
            &p.driver,
            &p.comment,
            &p.location,
            &p.port,
            &p.default_form,
        ] {
            for byte in part.bytes().chain([0]) {
                hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
            }
        }
        hash
    }
}

/// PRINTER_INFO at `level` (0 is PRINTER_INFO_STRESS); `None` for a level
/// not served.
pub(crate) fn printer(named: &Named<'_>, level: u32) -> Option<Block> {
    let p = named.printer;
    Some(match level {
        0 => {
            let mut b = Block::new(124);
            b.string(0, &named.printer_unc())
                .string(4, &named.server_unc())
                .u32(8, p.jobs)
                // dwGetVersion: the version srvsvc reports, 10.0.
                .u32(44, 0x0000_000A)
                .u32(76, 1)
                .u32(88, named.change_id())
                // PROCESSOR_ARCHITECTURE_AMD64.
                .u16(108, 9);
            b
        }
        1 => {
            let mut b = Block::new(16);
            b.u32(0, PRINTER_ENUM_ICON8)
                .string(
                    4,
                    &format!("{},{},{}", named.printer_unc(), p.driver, p.comment),
                )
                .string(8, &named.printer_unc())
                .string(12, &p.comment);
            b
        }
        2 => {
            let mut b = Block::new(84);
            b.string(0, &named.server_unc())
                .string(4, &named.printer_unc())
                .string(8, &p.share)
                .string(12, &p.port)
                .string(16, &p.driver)
                .string(20, &p.comment)
                .string(24, &p.location)
                .blob(28, devmode(&p.share, &p.default_form))
                .string(32, "")
                .string(36, PRINT_PROCESSOR)
                .string(40, RAW)
                .string(44, "")
                .blob(48, security_descriptor())
                .u32(52, PRINTER_ATTRIBUTES)
                .u32(56, 1)
                .u32(60, 1)
                .u32(76, p.jobs);
            b
        }
        3 => {
            let mut b = Block::new(4);
            b.blob(0, security_descriptor());
            b
        }
        4 => {
            let mut b = Block::new(12);
            b.string(0, &named.printer_unc())
                .string(4, &named.server_unc())
                .u32(8, PRINTER_ATTRIBUTES);
            b
        }
        5 => {
            let mut b = Block::new(20);
            b.string(0, &named.printer_unc())
                .string(4, &p.port)
                .u32(8, PRINTER_ATTRIBUTES)
                .u32(12, 15_000)
                .u32(16, 45_000);
            b
        }
        7 => {
            let mut b = Block::new(8);
            b.u32(4, DSPRINT_UNPUBLISH);
            b
        }
        8 => {
            let mut b = Block::new(4);
            b.blob(0, devmode(&p.share, &p.default_form));
            b
        }
        _ => return None,
    })
}

/// Size of DRIVER_INFO at `level`, for the levels served.
fn driver_size(level: u32) -> Option<usize> {
    Some(match level {
        1 => 4,
        2 => 24,
        3 => 40,
        4 => 44,
        6 => 80,
        8 => 120,
        _ => return None,
    })
}

/// DRIVER_INFO at `level` for the driver `name` in `environment`, with
/// `files` when it has some there.
pub(crate) fn driver(
    level: u32,
    name: &str,
    environment: &str,
    server: &str,
    files: Option<&DriverFiles>,
) -> Option<Block> {
    let mut b = Block::new(driver_size(level)?);
    if level == 1 {
        b.string(0, name);
        return Some(b);
    }
    let path = |file: &str| match file {
        "" => String::new(),
        file => format!("\\\\{server}\\print$\\{file}"),
    };
    let (version, driver_path, data, config, help, deps) = match files {
        Some(f) => (
            f.version,
            path(&f.driver_path),
            path(&f.data_file),
            path(&f.config_file),
            path(&f.help_file),
            f.dependent_files.iter().map(|d| path(d)).collect(),
        ),
        None => (
            DRIVER_VERSION_3,
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            Vec::new(),
        ),
    };
    b.u32(0, version)
        .string(4, name)
        .string(8, environment)
        .string(12, &driver_path)
        .string(16, &data)
        .string(20, &config);
    if level == 2 {
        return Some(b);
    }
    b.string(24, &help)
        .multi_sz(28, &deps)
        .string(32, "")
        .string(36, RAW);
    if level >= 4 {
        b.multi_sz(40, &[]);
    }
    if level >= 6 {
        // No date or version, manufacturer, URL, hardware ID or provider.
        b.u64(44, 0)
            .u64(56, 0)
            .string(64, "")
            .string(68, "")
            .string(72, "")
            .string(76, "");
    }
    if level == 8 {
        b.string(80, PRINT_PROCESSOR)
            .string(84, "")
            .multi_sz(88, &[])
            .string(92, "")
            .u32(96, 0)
            .multi_sz(100, &[])
            .u64(104, 0)
            .u64(112, 0);
    }
    Some(b)
}

/// JOB_INFO at `level` (1 or 2) of `job` on `named`.
pub(crate) fn job(named: &Named<'_>, job: &JobInfo, position: u32, level: u32) -> Option<Block> {
    let p = named.printer;
    let submitted = system_time(job.submitted);
    Some(match level {
        1 => {
            let mut b = Block::new(64);
            b.u32(0, job.id)
                .string(4, &named.printer_unc())
                .string(8, &job.machine)
                .string(12, &job.user)
                .string(16, &job.document)
                .string(20, RAW)
                .string(24, &job.status_text)
                .u32(28, job.status)
                .u32(32, 1)
                .u32(36, position)
                .u32(40, job.pages)
                .u32(44, 0)
                .bytes(48, &submitted);
            b
        }
        2 => {
            let mut b = Block::new(104);
            b.u32(0, job.id)
                .string(4, &named.printer_unc())
                .string(8, &job.machine)
                .string(12, &job.user)
                .string(16, &job.document)
                .string(20, &job.user)
                .string(24, RAW)
                .string(28, PRINT_PROCESSOR)
                .string(32, "")
                .string(36, &p.driver)
                .string(44, &job.status_text)
                .u32(52, job.status)
                .u32(56, 1)
                .u32(60, position)
                .u32(72, job.pages)
                .u32(76, job.size)
                .bytes(80, &submitted)
                .u32(96, 0)
                .u32(100, 0);
            b
        }
        _ => return None,
    })
}

/// A SYSTEMTIME in UTC (MS-DTYP 2.3.13): year, month, weekday, day, hour,
/// minute, second, milliseconds, 16 bits each.
pub(crate) fn system_time(at: SystemTime) -> [u8; 16] {
    let since = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs();
    let days = secs / 86_400;
    let rem = secs % 86_400;
    // Days to a civil date (proleptic Gregorian), from 1970-01-01.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    // 1970-01-01 was a Thursday (4).
    let weekday = (days + 4) % 7;
    let fields = [
        year as u64,
        month as u64,
        weekday,
        day as u64,
        rem / 3600,
        rem / 60 % 60,
        rem % 60,
        u64::from(since.subsec_millis()),
    ];
    let mut out = [0; 16];
    for (i, f) in fields.into_iter().enumerate() {
        let v = u16::try_from(f).unwrap_or(u16::MAX);
        out[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
    }
    out
}

/// DEVMODE size with no driver extra data (MS-RPRN 2.2.2.1).
pub(crate) const DEVMODE_SIZE: usize = 220;

/// The public DEVMODE a printer's documents start with: the printer's
/// name, portrait, one copy, the default form.
pub(crate) fn devmode(device: &str, form_name: &str) -> Vec<u8> {
    const DM_ORIENTATION: u32 = 0x0000_0001;
    const DM_PAPERSIZE: u32 = 0x0000_0002;
    const DM_COPIES: u32 = 0x0000_0100;
    const DM_FORMNAME: u32 = 0x0001_0000;
    let form = forms::find(form_name).unwrap_or(&forms::FORMS[0]);
    let mut b = Block::new(DEVMODE_SIZE);
    b.bytes(0, &wchar32(device))
        .u16(64, 0x0401)
        .u16(66, 0)
        .u16(68, DEVMODE_SIZE as u16)
        .u16(70, 0)
        .u32(72, DM_ORIENTATION | DM_PAPERSIZE | DM_COPIES | DM_FORMNAME)
        .u16(76, 1)
        .u16(78, form.paper)
        .u16(86, 1)
        .bytes(102, &wchar32(form.name));
    super::marshal::pack(vec![b])
}

/// `text` in a WCHAR[32], cut to 31 characters and NUL-filled.
fn wchar32(text: &str) -> [u8; 64] {
    let mut out = [0; 64];
    let units = utf16z(text);
    let len = units.len().min(62);
    out[..len].copy_from_slice(&units[..len]);
    out
}

/// A self-relative security descriptor: owned by BUILTIN\Administrators,
/// with a DACL letting Everyone print and read it (READ_CONTROL |
/// PRINTER_ACCESS_USE).
pub(crate) fn security_descriptor() -> Vec<u8> {
    const SE_DACL_PRESENT: u16 = 0x0004;
    const SE_SELF_RELATIVE: u16 = 0x8000;
    let administrators = [1, 2, 0, 0, 0, 0, 0, 5, 32, 0, 0, 0, 0x20, 2, 0, 0];
    let everyone = [1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0];
    let mut ace = vec![0, 0];
    ace.extend_from_slice(&(8u16 + everyone.len() as u16).to_le_bytes());
    ace.extend_from_slice(&0x0002_0008u32.to_le_bytes());
    ace.extend_from_slice(&everyone);
    let mut acl = vec![2, 0];
    acl.extend_from_slice(&(8 + ace.len() as u16).to_le_bytes());
    acl.extend_from_slice(&[1, 0, 0, 0]);
    acl.extend(ace);
    let owner_at = 20u32;
    let dacl_at = owner_at + administrators.len() as u32;
    let mut sd = vec![1, 0];
    sd.extend_from_slice(&(SE_DACL_PRESENT | SE_SELF_RELATIVE).to_le_bytes());
    for at in [owner_at, 0, 0, dacl_at] {
        sd.extend_from_slice(&at.to_le_bytes());
    }
    sd.extend_from_slice(&administrators);
    sd.extend(acl);
    sd
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::super::marshal::pack;
    use super::*;

    fn printer_info() -> PrinterInfo {
        PrinterInfo {
            share: "PRT01".into(),
            driver: "Microsoft PCL6 Class Driver".into(),
            comment: "Second floor".into(),
            location: "Room 2".into(),
            port: "QUSRSYS/PRT01".into(),
            default_form: "A4".into(),
            jobs: 2,
            driver_files: Vec::new(),
        }
    }

    fn u32_at(buf: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(buf[at..at + 4].try_into().unwrap())
    }

    /// The string an offset at `at` in the block at `start` points to.
    fn string_at(buf: &[u8], start: usize, at: usize) -> String {
        let from = start + u32_at(buf, start + at) as usize;
        let units: Vec<u16> = buf[from..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .take_while(|&u| u != 0)
            .collect();
        String::from_utf16(&units).unwrap()
    }

    #[test]
    fn printer_info_2_names_the_share_driver_and_port() {
        let p = printer_info();
        let named = Named {
            server: "QSHQ400",
            printer: &p,
        };
        let out = pack(vec![printer(&named, 2).unwrap()]);
        assert_eq!(string_at(&out, 0, 0), "\\\\QSHQ400");
        assert_eq!(string_at(&out, 0, 4), "\\\\QSHQ400\\PRT01");
        assert_eq!(string_at(&out, 0, 8), "PRT01");
        assert_eq!(string_at(&out, 0, 12), "QUSRSYS/PRT01");
        assert_eq!(string_at(&out, 0, 16), "Microsoft PCL6 Class Driver");
        assert_eq!(string_at(&out, 0, 36), "winprint");
        assert_eq!(string_at(&out, 0, 40), "RAW");
        assert_eq!(u32_at(&out, 52), PRINTER_ATTRIBUTES);
        assert_eq!(u32_at(&out, 76), 2, "cJobs");
        let dm = u32_at(&out, 28) as usize;
        assert_eq!(dm % 4, 0);
        assert_eq!(&out[dm + 68..dm + 70], &220u16.to_le_bytes());
        assert_eq!(&out[dm + 78..dm + 80], &9u16.to_le_bytes(), "A4");
        let sd = u32_at(&out, 48) as usize;
        assert_eq!(sd % 4, 0);
        assert_eq!(&out[sd..sd + 4], &[1, 0, 0x04, 0x80]);
    }

    #[test]
    fn every_served_level_has_its_size() {
        let p = printer_info();
        let named = Named {
            server: "S",
            printer: &p,
        };
        for (level, size) in [
            (0, 124),
            (1, 16),
            (2, 84),
            (3, 4),
            (4, 12),
            (5, 20),
            (7, 8),
            (8, 4),
        ] {
            let out = pack(vec![printer(&named, level).unwrap()]);
            assert!(out.len() >= size, "{level}");
        }
        for level in [6, 9, 10] {
            assert!(printer(&named, level).is_none(), "{level}");
        }
        for (level, size) in [(1, 4), (2, 24), (3, 40), (4, 44), (6, 80), (8, 120)] {
            assert_eq!(driver_size(level), Some(size));
            assert!(driver(level, "D", "Windows x64", "S", None).is_some());
        }
        assert!(driver(5, "D", "Windows x64", "S", None).is_none());
    }

    #[test]
    fn driver_files_are_paths_in_print_dollar() {
        let files = DriverFiles {
            environment: "Windows x64".into(),
            version: 3,
            driver_path: "x64\\3\\unidrv.dll".into(),
            data_file: "x64\\3\\model.gpd".into(),
            config_file: "x64\\3\\unidrvui.dll".into(),
            help_file: String::new(),
            dependent_files: vec!["x64\\3\\stdnames.gpd".into()],
        };
        let out = pack(vec![
            driver(8, "Model", "Windows x64", "QS", Some(&files)).unwrap(),
        ]);
        assert_eq!(u32_at(&out, 0), 3);
        assert_eq!(string_at(&out, 0, 12), "\\\\QS\\print$\\x64\\3\\unidrv.dll");
        assert_eq!(string_at(&out, 0, 24), "");
        assert_eq!(
            string_at(&out, 0, 28),
            "\\\\QS\\print$\\x64\\3\\stdnames.gpd"
        );
        assert_eq!(string_at(&out, 0, 80), "winprint");
        assert_eq!(u32_at(&out, 40), 0, "no previous names");
    }

    #[test]
    fn system_times_are_civil_utc() {
        // 2026-11-09 21:27:05.250, a Monday.
        let at = UNIX_EPOCH + Duration::from_millis(1_794_259_625_250);
        let st = system_time(at);
        let f: Vec<u16> = st
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        assert_eq!(f, [2026, 11, 1, 9, 21, 27, 5, 250]);
        let leap = system_time(UNIX_EPOCH + Duration::from_secs(951_782_400));
        assert_eq!(&leap[..8], &[0xD0, 0x07, 2, 0, 2, 0, 29, 0], "2000-02-29");
    }

    #[test]
    fn the_security_descriptor_is_self_relative() {
        let sd = security_descriptor();
        assert_eq!(u32_at(&sd, 4), 20, "owner");
        assert_eq!(u32_at(&sd, 16), 36, "dacl");
        assert_eq!(sd.len(), 36 + 8 + 20);
        assert_eq!(u32_at(&sd, 36 + 8 + 4), 0x0002_0008);
    }
}
