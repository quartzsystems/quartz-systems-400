// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A display-file source member split the way SDA works on it: the
//! file-level lines, then each record format with its own lines and
//! its fields and constants, every piece keeping the source lines it
//! came from so an unchanged piece is written back as it was.

use dds::lex::{Keyword, NameType, Spec};
use dds::{Source, diag::Diagnostics};

use super::line::Line;

/// The record types SDA offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordType {
    /// An ordinary record.
    Record,
    /// `USRDFN`.
    UserDefined,
    /// A subfile record (`SFL`).
    Subfile,
    /// A subfile control record (`SFLCTL`).
    SubfileControl,
    /// A message subfile record (`SFLMSGRCD`).
    MessageSubfile,
    /// A window record (`WINDOW` without `SFLCTL`).
    Window,
    /// A window subfile control record (`WINDOW` with `SFLCTL`).
    WindowSubfile,
    /// A pull-down (`PULLDOWN`).
    PullDown,
    /// A pull-down subfile control record.
    PullDownSubfile,
    /// A menu bar (`MNUBAR`).
    MenuBar,
}

impl RecordType {
    /// The word SDA shows and accepts on Add New Record.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Record => "RECORD",
            Self::UserDefined => "USRDFN",
            Self::Subfile => "SFL",
            Self::SubfileControl => "SFLCTL",
            Self::MessageSubfile => "SFLMSG",
            Self::Window => "WINDOW",
            Self::WindowSubfile => "WDWSFL",
            Self::PullDown => "PULDWN",
            Self::PullDownSubfile => "PDNSFL",
            Self::MenuBar => "MNUBAR",
        }
    }

    /// From the word typed on Add New Record.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        Some(match w.trim().to_ascii_uppercase().as_str() {
            "RECORD" => Self::Record,
            "USRDFN" => Self::UserDefined,
            "SFL" => Self::Subfile,
            "SFLMSG" => Self::MessageSubfile,
            "WINDOW" => Self::Window,
            "WDWSFL" => Self::WindowSubfile,
            "PULDWN" => Self::PullDown,
            "PDNSFL" => Self::PullDownSubfile,
            "MNUBAR" => Self::MenuBar,
            _ => return None,
        })
    }
}

/// A field's or constant's use, as the design image shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Usage {
    /// Output only (`O` or blank).
    Output,
    /// Input only (`I`).
    Input,
    /// Both (`B`).
    Both,
    /// Hidden (`H`).
    Hidden,
    /// Message (`M`).
    Message,
    /// Program-to-system (`P`).
    Program,
}

impl Usage {
    /// From DDS column 38.
    #[must_use]
    pub fn from_letter(c: char) -> Self {
        match c {
            'I' => Self::Input,
            'B' => Self::Both,
            'H' => Self::Hidden,
            'M' => Self::Message,
            'P' => Self::Program,
            _ => Self::Output,
        }
    }

    /// The letter for column 38.
    #[must_use]
    pub fn letter(self) -> char {
        match self {
            Self::Output => 'O',
            Self::Input => 'I',
            Self::Both => 'B',
            Self::Hidden => 'H',
            Self::Message => 'M',
            Self::Program => 'P',
        }
    }

    /// Whether the field has a place on the display.
    #[must_use]
    pub fn displayed(self) -> bool {
        matches!(self, Self::Output | Self::Input | Self::Both)
    }
}

/// What an element is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A constant: `'text'`, or a keyword constant (`DATE`, `TIME`,
    /// `USER`, `SYSNAME`, `MSGCON`, `DFT`) shown by its text.
    Constant {
        /// The text shown on the image.
        text: String,
        /// The keyword when it is not a plain `'text'`.
        keyword: Option<String>,
    },
    /// A named field.
    Field {
        /// The name.
        name: String,
        /// The use.
        usage: Usage,
        /// The data type letter (blank for the default).
        ty: char,
        /// Length.
        len: u32,
        /// Decimal positions; `None` for character.
        dec: Option<u8>,
        /// `R` in column 29: the shape comes from a reference.
        reference: bool,
    },
}

/// A field or constant of a record with its source lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    /// What it is.
    pub kind: Kind,
    /// Row and column; `None` for a hidden field or a relative one not
    /// yet resolved.
    pub pos: Option<(u16, u16)>,
    /// The positions it takes on the display.
    pub width: u32,
    /// The display attributes given by `DSPATR` without a condition.
    pub attrs: Vec<String>,
    /// The colors given by `COLOR` without a condition.
    pub colors: Vec<String>,
    /// Option indicators condition the element.
    pub conditioned: bool,
    /// The keywords on its lines, as lexed.
    pub keywords: Vec<Keyword>,
    /// The source lines, comments in front included.
    pub lines: Vec<Line>,
    /// The element as it was read; `None` for one added since.
    pub orig: Option<Box<Snapshot>>,
    /// The member line it was read from (its first non-comment line).
    pub src_line: Option<usize>,
    /// The member line of `lines[0]`.
    pub base: usize,
}

/// What an element was when it was read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// The kind.
    pub kind: Kind,
    /// The position.
    pub pos: Option<(u16, u16)>,
    /// The attributes.
    pub attrs: Vec<String>,
    /// The colors.
    pub colors: Vec<String>,
    /// The keywords.
    pub keywords: Vec<Keyword>,
}

impl Element {
    /// A new element with no source yet.
    #[must_use]
    pub fn new(kind: Kind, pos: Option<(u16, u16)>) -> Self {
        let width = natural_width(&kind, &[]);
        Self {
            kind,
            pos,
            width,
            attrs: Vec::new(),
            colors: Vec::new(),
            conditioned: false,
            keywords: Vec::new(),
            lines: Vec::new(),
            orig: None,
            src_line: None,
            base: 0,
        }
    }

    /// A new field as the design image places it: character or
    /// numeric (zoned, `dec` decimal positions); a numeric output field
    /// with decimals gets SDA's edit word.
    #[must_use]
    pub fn field(
        name: &str,
        usage: Usage,
        numeric: bool,
        len: u32,
        dec: u8,
        pos: Option<(u16, u16)>,
    ) -> Self {
        let kind = Kind::Field {
            name: name.to_ascii_uppercase(),
            usage,
            ty: ' ',
            len,
            dec: numeric.then_some(dec),
            reference: false,
        };
        let mut e = Self::new(kind, pos);
        if numeric && dec > 0 && usage == Usage::Output {
            e.keywords.push(Keyword {
                name: "EDTWRD".into(),
                parms: vec![dds::lex::Parm::Quoted(super::write::edtwrd(len, dec))],
                line: 0,
            });
        }
        e.width = natural_width(&e.kind, &e.keywords);
        e
    }

    /// Remember the element as it stands as the read state.
    pub fn settle(&mut self) {
        self.orig = Some(Box::new(Snapshot {
            kind: self.kind.clone(),
            pos: self.pos,
            attrs: self.attrs.clone(),
            colors: self.colors.clone(),
            keywords: self.keywords.clone(),
        }));
    }

    /// Whether it differs from what was read (or was added).
    #[must_use]
    pub fn is_changed(&self) -> bool {
        match &self.orig {
            None => true,
            Some(o) => {
                o.kind != self.kind
                    || o.pos != self.pos
                    || o.attrs != self.attrs
                    || o.colors != self.colors
                    || o.keywords != self.keywords
            }
        }
    }

    /// The field name, or `None` for a constant.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match &self.kind {
            Kind::Field { name, .. } => Some(name),
            Kind::Constant { .. } => None,
        }
    }

    /// Whether this is a constant.
    #[must_use]
    pub fn is_constant(&self) -> bool {
        matches!(self.kind, Kind::Constant { .. })
    }

    /// Whether it shows on the display.
    #[must_use]
    pub fn displayed(&self) -> bool {
        match &self.kind {
            Kind::Constant { .. } => true,
            Kind::Field { usage, .. } => usage.displayed(),
        }
    }
}

/// One record format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The name.
    pub name: String,
    /// The type.
    pub ty: RecordType,
    /// The record a control record controls or the subfile names.
    pub related: Option<String>,
    /// The record-level keywords, as lexed.
    pub keywords: Vec<Keyword>,
    /// The `R` line, the record-level lines and its comments.
    pub head: Vec<Line>,
    /// The fields and constants in source order.
    pub elements: Vec<Element>,
    /// Comment lines after the last element.
    pub tail: Vec<Line>,
    /// Elements were added, deleted or reordered, or the record's own
    /// lines changed, since it was read.
    pub changed: bool,
    /// The record-level keywords as read; `None` for a new record.
    pub orig_keywords: Option<Vec<Keyword>>,
    /// The member line of `head[0]`.
    pub head_base: usize,
    /// The `yymmdd` date shown on the record list.
    pub date: String,
}

impl Record {
    /// An empty record of a type.
    #[must_use]
    pub fn new(name: &str, ty: RecordType) -> Self {
        Self {
            name: name.to_ascii_uppercase(),
            ty,
            related: None,
            keywords: Vec::new(),
            head: Vec::new(),
            elements: Vec::new(),
            tail: Vec::new(),
            changed: true,
            orig_keywords: None,
            head_base: 0,
            date: String::new(),
        }
    }

    /// The record-level keyword called `name`.
    #[must_use]
    pub fn keyword(&self, name: &str) -> Option<&Keyword> {
        self.keywords.iter().find(|k| k.name == name)
    }

    /// Whether anything in the record changed since it was read.
    #[must_use]
    pub fn is_changed(&self) -> bool {
        self.changed
            || self.orig_keywords.as_ref() != Some(&self.keywords)
            || self.elements.iter().any(Element::is_changed)
    }

    /// The element at a position, if one covers it.
    #[must_use]
    pub fn element_at(&self, row: u16, col: u16) -> Option<usize> {
        self.elements.iter().position(|e| match e.pos {
            Some((r, c)) => r == row && col >= c && u32::from(col) < u32::from(c) + e.width.max(1),
            None => false,
        })
    }

    /// The next free generated field name (`FLD001` on).
    #[must_use]
    pub fn next_field_name(&self, taken: &[String]) -> String {
        (1..=99_999)
            .map(|n| format!("FLD{n:03}"))
            .find(|n| !taken.contains(n) && !self.elements.iter().any(|e| e.name() == Some(n)))
            .unwrap_or_default()
    }
}

/// A source member.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Member {
    /// The file-level lines (comments and keywords before the first
    /// record).
    pub head: Vec<Line>,
    /// The file-level keywords, as lexed.
    pub keywords: Vec<Keyword>,
    /// The records in source order.
    pub records: Vec<Record>,
    /// The file-level keywords as read; `None` for a new member.
    pub orig_keywords: Option<Vec<Keyword>>,
}

impl Member {
    /// The record called `name`.
    #[must_use]
    pub fn record(&self, name: &str) -> Option<usize> {
        let n = name.trim().to_ascii_uppercase();
        self.records.iter().position(|r| r.name == n)
    }

    /// Field names of every record.
    #[must_use]
    pub fn field_names(&self) -> Vec<String> {
        self.records
            .iter()
            .flat_map(|r| r.elements.iter().filter_map(|e| e.name().map(str::to_string)))
            .collect()
    }

    /// The file-level keyword called `name`.
    #[must_use]
    pub fn keyword(&self, name: &str) -> Option<&Keyword> {
        self.keywords.iter().find(|k| k.name == name)
    }
}

fn word_kw(name: &str, parms: &[&str]) -> Keyword {
    Keyword {
        name: name.into(),
        parms: parms.iter().map(|p| dds::lex::Parm::Word((*p).into())).collect(),
        line: 0,
    }
}

/// The keywords a new record of a type starts with; a control record
/// controls the member's last subfile record.
#[must_use]
pub fn type_keywords(ty: RecordType, m: &Member) -> Vec<Keyword> {
    let last_sfl = m
        .records
        .iter()
        .rev()
        .find(|r| r.ty == RecordType::Subfile)
        .map(|r| r.name.clone())
        .unwrap_or_default();
    let window = ["2", "10", "10", "50"];
    match ty {
        RecordType::Record => Vec::new(),
        RecordType::UserDefined => vec![word_kw("USRDFN", &[])],
        RecordType::Subfile => vec![word_kw("SFL", &[])],
        RecordType::SubfileControl => vec![
            word_kw("SFLCTL", &[&last_sfl]),
            word_kw("SFLSIZ", &["0010"]),
            word_kw("SFLPAG", &["0009"]),
            word_kw("SFLDSP", &[]),
            word_kw("SFLDSPCTL", &[]),
        ],
        RecordType::MessageSubfile => vec![word_kw("SFL", &[]), word_kw("SFLMSGRCD", &["24"])],
        RecordType::Window => vec![word_kw("WINDOW", &window)],
        RecordType::WindowSubfile => vec![word_kw("SFLCTL", &[&last_sfl]), word_kw("WINDOW", &window)],
        RecordType::PullDown => vec![word_kw("PULLDOWN", &[])],
        RecordType::PullDownSubfile => vec![word_kw("SFLCTL", &[&last_sfl]), word_kw("PULLDOWN", &[])],
        RecordType::MenuBar => vec![word_kw("MNUBAR", &[])],
    }
}

/// The records the display-file compiler finds errors in (severity 20
/// and up), by name.
#[must_use]
pub fn errors(lines: &[String]) -> Vec<String> {
    let (src, _) = Source::from_text(&lines.join("\n"));
    let c = dspf::compile_dspf(&src, &dds::NoRefs, &dds::CompileOpts::default());
    let mut bad = Vec::new();
    for d in c.diagnostics.iter().filter(|d| d.severity >= dds::Severity::Error) {
        let Some(at) = d.line else { continue };
        let upto = at.min(lines.len().saturating_sub(1));
        let name = lines[..=upto]
            .iter()
            .rev()
            .map(|l| Line::new(l))
            .find(|l| !l.is_comment() && l.name_type() == 'R')
            .map(|l| l.name());
        if let Some(n) = name
            && !bad.contains(&n)
        {
            bad.push(n);
        }
    }
    bad
}

/// Read a member's lines.
#[must_use]
pub fn parse(lines: &[String]) -> Member {
    let text = lines.join("\n");
    let (source, _) = Source::from_text(&text);
    let mut diags = Diagnostics::default();
    let specs = dds::lex::lex(&source, &mut diags);
    let all: Vec<Line> = lines.iter().map(|l| Line::new(l)).collect();
    let ranges = ranges(&specs, &all);
    let mut m = Member::default();
    let mut first_record = specs.len();
    for (i, s) in specs.iter().enumerate() {
        if s.name_type == Some(NameType::Record) {
            first_record = i;
            break;
        }
    }
    // File level: everything before the first record.
    let head_end = specs
        .get(first_record)
        .map_or(all.len(), |s| ranges[first_record].0.min(s.line));
    m.head = all[..head_end].to_vec();
    for s in &specs[..first_record] {
        m.keywords.extend(s.keywords.iter().cloned());
    }
    if !lines.is_empty() {
        m.orig_keywords = Some(m.keywords.clone());
    }
    let mut cur: Option<Record> = None;
    for (i, s) in specs.iter().enumerate().skip(first_record) {
        let (from, to) = ranges[i];
        let chunk = all[from..to].to_vec();
        if s.name_type == Some(NameType::Record) {
            if let Some(r) = cur.take() {
                m.records.push(finish(r));
            }
            let name = s.name.clone().unwrap_or_default();
            let mut r = Record::new(&name, RecordType::Record);
            r.changed = false;
            r.keywords.clone_from(&s.keywords);
            r.orig_keywords = Some(s.keywords.clone());
            r.head_base = from;
            r.head = chunk;
            cur = Some(r);
            continue;
        }
        let Some(r) = cur.as_mut() else { continue };
        r.elements.push(element(s, chunk, from));
    }
    if let Some(r) = cur.take() {
        m.records.push(finish(r));
    }
    resolve_relative(&mut m);
    measure(&mut m, &source);
    for r in &mut m.records {
        for e in &mut r.elements {
            e.settle();
        }
    }
    m
}

/// Widths and positions as the display-file compiler works them out,
/// for a member it accepts.
fn measure(m: &mut Member, source: &Source) {
    let c = dspf::compile_dspf(source, &dds::NoRefs, &dds::CompileOpts::default());
    let Some(def) = c.definition else { return };
    for rec in &def.records {
        let Some(ri) = m.record(&rec.name) else { continue };
        for f in &rec.fields {
            let r = &mut m.records[ri];
            let Some(e) = r.elements.iter_mut().find(|e| e.src_line == Some(f.line)) else {
                continue;
            };
            e.width = f.screen_len;
            if let Some(p) = f.position(0) {
                e.pos = Some((u16::from(p.row), u16::from(p.col)));
            }
        }
    }
}

/// The source line range of each spec: from its first line (comments
/// in front of it included, `%%` stamps excepted) to the next one's.
fn ranges(specs: &[Spec], all: &[Line]) -> Vec<(usize, usize)> {
    let mut starts: Vec<usize> = specs.iter().map(|s| s.line).collect();
    // Pull leading comments onto the spec that follows them.
    for i in 1..starts.len() {
        let floor = specs[i - 1].line + 1;
        let mut s = starts[i];
        while s > floor && all[s - 1].is_comment() && !all[s - 1].get(8, 9).eq("%%") {
            s -= 1;
        }
        starts[i] = s;
    }
    (0..specs.len())
        .map(|i| {
            let end = starts.get(i + 1).copied().unwrap_or(all.len());
            (starts[i], end)
        })
        .collect()
}

/// A record with its type and tail worked out.
fn finish(mut r: Record) -> Record {
    let has = |k: &str| r.keywords.iter().any(|kw| kw.name == k);
    r.ty = if has("SFLCTL") {
        r.related = r.keyword("SFLCTL").and_then(|k| k.word(0)).map(str::to_string);
        if has("WINDOW") {
            RecordType::WindowSubfile
        } else if has("PULLDOWN") {
            RecordType::PullDownSubfile
        } else {
            RecordType::SubfileControl
        }
    } else if has("SFLMSGRCD") {
        RecordType::MessageSubfile
    } else if has("SFL") {
        RecordType::Subfile
    } else if has("WINDOW") {
        RecordType::Window
    } else if has("PULLDOWN") {
        RecordType::PullDown
    } else if has("MNUBAR") {
        RecordType::MenuBar
    } else if has("USRDFN") {
        RecordType::UserDefined
    } else {
        RecordType::Record
    };
    // Trailing comments of the last element belong to the record.
    if let Some(last) = r.elements.last_mut() {
        while last.lines.len() > 1 && last.lines.last().is_some_and(Line::is_comment) {
            let l = last.lines.pop().expect("a line");
            r.tail.insert(0, l);
        }
    }
    r
}

/// An element from its spec and lines; `from` is the global index of
/// the first of them.
fn element(s: &Spec, lines: Vec<Line>, from: usize) -> Element {
    let (rw, cw) = s
        .line
        .checked_sub(from)
        .and_then(|i| lines.get(i))
        .map(Line::location)
        .unwrap_or_default();
    let pos = match (rw.parse::<u16>(), cw.parse::<u16>()) {
        (Ok(r), Ok(c)) => Some((r, c)),
        _ => None,
    };
    // A keyword on a line with indicators is conditioned.
    let line_cond = |k: &Keyword| -> bool {
        k.line
            .checked_sub(from)
            .and_then(|i| lines.get(i))
            .is_some_and(|l| !l.blank(8, 16))
    };
    let mut attrs = Vec::new();
    let mut colors = Vec::new();
    for k in &s.keywords {
        if line_cond(k) {
            continue;
        }
        let words = k.parms.iter().filter_map(|p| p.as_word().map(str::to_string));
        match k.name.as_str() {
            "DSPATR" => attrs.extend(words),
            "COLOR" => colors.extend(words),
            _ => {}
        }
    }
    let kind = match &s.name {
        Some(name) => Kind::Field {
            name: name.clone(),
            usage: Usage::from_letter(s.usage.unwrap_or(' ')),
            ty: s.data_type.unwrap_or(' '),
            len: s.length.unwrap_or(0),
            dec: s.decimals,
            reference: s.reference,
        },
        None => constant(&s.keywords),
    };
    let width = natural_width(&kind, &s.keywords);
    Element {
        kind,
        pos,
        width,
        attrs,
        colors,
        conditioned: !s.conditioning.is_empty(),
        keywords: s.keywords.clone(),
        lines,
        orig: None,
        src_line: Some(s.line),
        base: from,
    }
}

/// The positions an element takes, without the compiler: a constant's
/// characters; a field's edit word, or its length, with room for the
/// sign and the decimal point on a numeric input field.
#[must_use]
pub fn natural_width(kind: &Kind, keywords: &[Keyword]) -> u32 {
    match kind {
        Kind::Constant { text, .. } => u32::try_from(text.chars().count()).unwrap_or(0),
        Kind::Field {
            len, dec, usage, ty, ..
        } => {
            if let Some(w) = keywords
                .iter()
                .find(|k| k.name == "EDTWRD")
                .and_then(|k| k.quoted(0))
            {
                return u32::try_from(w.chars().count()).unwrap_or(*len);
            }
            let numeric = dec.is_some() || matches!(ty, 'S' | 'Y' | 'P' | 'B' | 'F');
            if numeric && matches!(usage, Usage::Input | Usage::Both) {
                *len + 1 + u32::from(dec.unwrap_or(0) > 0)
            } else {
                *len
            }
        }
    }
}

/// A constant's text from its keywords.
fn constant(keywords: &[Keyword]) -> Kind {
    for k in keywords {
        match k.name.as_str() {
            dds::lex::keyword::CONSTANT => {
                return Kind::Constant {
                    text: k.quoted(0).unwrap_or_default().to_string(),
                    keyword: None,
                };
            }
            "DATE" => return keyword_constant("DATE", "MM/DD/YY", k),
            "TIME" => return keyword_constant("TIME", "HH:MM:SS", k),
            "USER" => return keyword_constant("USER", "USERNAME  ", k),
            "SYSNAME" => return keyword_constant("SYSNAME", "SYSNAME ", k),
            "DFT" => {
                return Kind::Constant {
                    text: k.quoted(0).unwrap_or_default().to_string(),
                    keyword: Some("DFT".into()),
                };
            }
            "MSGCON" => {
                let n: usize = k.word(0).and_then(|w| w.parse().ok()).unwrap_or(1);
                return Kind::Constant {
                    text: "M".repeat(n),
                    keyword: Some("MSGCON".into()),
                };
            }
            _ => {}
        }
    }
    Kind::Constant {
        text: String::new(),
        keyword: Some(String::new()),
    }
}

fn keyword_constant(name: &str, shown: &str, _k: &Keyword) -> Kind {
    Kind::Constant {
        text: shown.to_string(),
        keyword: Some(name.into()),
    }
}

/// `+n` columns and rows given on the line before: worked out from the
/// element before in the record.
fn resolve_relative(m: &mut Member) {
    for r in &mut m.records {
        let mut prev_end: Option<(u16, u16)> = None;
        for e in &mut r.elements {
            let first = e.lines.iter().find(|l| !l.is_comment());
            let (rw, cw) = first.map(Line::location).unwrap_or_default();
            if e.pos.is_none() && e.displayed() {
                let row = rw.parse::<u16>().ok().or(prev_end.map(|p| p.0));
                let col = cw
                    .strip_prefix('+')
                    .and_then(|n| n.parse::<u16>().ok())
                    .and_then(|n| prev_end.map(|p| p.1 + n + 1));
                if let (Some(row), Some(col)) = (row, col) {
                    e.pos = Some((row, col));
                }
            }
            if let Some((row, col)) = e.pos {
                let w = u16::try_from(e.width).unwrap_or(0);
                prev_end = Some((row, col + w.saturating_sub(1)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_columns_and_types() {
        let src: Vec<String> = [
            "     A                                      DSPSIZ(24 80 *DS3)",
            "     A          R ONE",
            "     A                                  2  2'Name'",
            "     A            NAME          10A  B   +2",
            "     A            QTY            5Y 0B  4  2",
            "     A            SECRET         8A  H",
            "     A          R TWO                       SFL",
            "     A            X              1A  O  6  2",
            "     A          R TWOCTL                    SFLCTL(TWO)",
            "     A                                      SFLSIZ(10) SFLPAG(5)",
            "     A                                      SFLDSP",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        let m = parse(&src);
        let one = &m.records[0];
        assert_eq!(one.elements[1].pos, Some((2, 8)));
        assert_eq!(one.elements[2].width, 6);
        assert!(!one.elements[3].displayed());
        assert_eq!(m.records[1].ty, RecordType::Subfile);
        assert_eq!(m.records[2].ty, RecordType::SubfileControl);
        assert_eq!(m.records[2].related.as_deref(), Some("TWO"));
        assert_eq!(m.field_names(), ["NAME", "QTY", "SECRET", "X"]);
    }
}
