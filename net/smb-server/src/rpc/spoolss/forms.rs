//! The forms every printer has (MS-RPRN 2.2.2.5, RpcEnumForms and
//! RpcGetForm): Windows' built-in names and sizes, in thousandths of a
//! millimetre, with the whole sheet imageable.

use super::marshal::Block;

/// FORM_BUILTIN (MS-RPRN 2.2.2.5.1).
const FORM_BUILTIN: u32 = 0x0000_0001;

/// STRING_NONE: the display name is the name (MS-RPRN 2.2.2.5.2).
const STRING_NONE: u32 = 0x0000_0001;

pub(crate) struct Form {
    pub name: &'static str,
    /// The PPD keyword of FORM_INFO_2.
    pub keyword: &'static str,
    pub width: u32,
    pub height: u32,
    /// DMPAPER_* for a DEVMODE's dmPaperSize.
    pub paper: u16,
}

pub(crate) const FORMS: [Form; 10] = [
    form("Letter", "Letter", 215_900, 279_400, 1),
    form("Legal", "Legal", 215_900, 355_600, 5),
    form("Tabloid", "Tabloid", 279_400, 431_800, 3),
    form("Executive", "Executive", 184_150, 266_700, 7),
    form("A3", "A3", 297_000, 420_000, 8),
    form("A4", "A4", 210_000, 297_000, 9),
    form("A5", "A5", 148_000, 210_000, 11),
    form("B5 (JIS)", "B5", 182_000, 257_000, 13),
    form("Envelope #10", "Env10", 104_775, 241_300, 20),
    form("Envelope DL", "EnvDL", 110_000, 220_000, 27),
];

const fn form(
    name: &'static str,
    keyword: &'static str,
    width: u32,
    height: u32,
    paper: u16,
) -> Form {
    Form {
        name,
        keyword,
        width,
        height,
        paper,
    }
}

/// The form named `name`, ignoring case.
pub(crate) fn find(name: &str) -> Option<&'static Form> {
    FORMS.iter().find(|f| f.name.eq_ignore_ascii_case(name))
}

/// FORM_INFO_1 or FORM_INFO_2 of `form`; `None` for another level.
pub(crate) fn info(form: &Form, level: u32) -> Option<Block> {
    let size = match level {
        1 => 32,
        2 => 56,
        _ => return None,
    };
    let mut b = Block::new(size);
    b.u32(0, FORM_BUILTIN)
        .string(4, form.name)
        .u32(8, form.width)
        .u32(12, form.height)
        .u32(16, 0)
        .u32(20, 0)
        .u32(24, form.width)
        .u32(28, form.height);
    if level == 2 {
        b.ansi(32, form.keyword)
            .u32(36, STRING_NONE)
            .u32(44, 0)
            .string(48, form.name)
            .u16(52, 0);
    }
    Some(b)
}

#[cfg(test)]
mod tests {
    use super::super::marshal::pack;
    use super::*;

    #[test]
    fn letter_and_a4_are_there_by_any_case() {
        assert_eq!(find("letter").unwrap().width, 215_900);
        assert_eq!(find("A4").unwrap().paper, 9);
        assert!(find("Letter Small").is_none());
    }

    #[test]
    fn form_info_1_is_the_sheet_and_its_name() {
        let out = pack(vec![info(&FORMS[5], 1).unwrap()]);
        assert_eq!(out.len(), 32 + 6);
        assert_eq!(&out[..4], &[1, 0, 0, 0]);
        assert_eq!(&out[4..8], &32u32.to_le_bytes());
        assert_eq!(&out[8..12], &210_000u32.to_le_bytes());
        assert_eq!(&out[28..32], &297_000u32.to_le_bytes());
        assert_eq!(&out[32..], &[b'A', 0, b'4', 0, 0, 0]);
    }

    #[test]
    fn form_info_2_adds_the_keyword_and_display_name() {
        let out = pack(vec![info(&FORMS[0], 2).unwrap()]);
        let at = |i: usize| u32::from_le_bytes(out[i..i + 4].try_into().unwrap()) as usize;
        assert_eq!(&out[at(32)..at(32) + 7], b"Letter\0");
        assert_eq!(at(36), 1);
        assert_eq!(at(40), 0, "no MUI DLL");
        assert_eq!(&out[at(48)..at(48) + 2], &[b'L', 0]);
        assert!(info(&FORMS[0], 3).is_none());
    }
}
