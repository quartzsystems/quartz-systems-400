// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the APIs need from the system: objects, data queues, data
//! areas, user indexes, files, system values, commands and the clock.
//! The service console implements [`Provider`] over the store;
//! [`MemProvider`] holds everything in memory for tests and the IR
//! driver.

use std::collections::{HashMap, VecDeque};

/// A failed API: the message identifier and its data, as the error
/// code parameter reports them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiFail {
    /// `CPFxxxx`.
    pub msgid: String,
    /// Substitution data, EBCDIC.
    pub data: Vec<u8>,
}

impl ApiFail {
    /// A failure with no data.
    #[must_use]
    pub fn id(msgid: &str) -> Self {
        Self {
            msgid: msgid.into(),
            data: Vec::new(),
        }
    }

    /// Append a blank-padded character field.
    #[must_use]
    pub fn text(mut self, s: &str, n: usize) -> Self {
        let mut b = ccsid::to_ebcdic(s).unwrap_or_default();
        b.resize(n, 0x40);
        self.data.extend(b);
        self
    }

    /// Append a binary field.
    #[must_use]
    pub fn bin4(mut self, v: i32) -> Self {
        self.data.extend(v.to_be_bytes());
        self
    }

    /// `CPF9801`: object not found.
    #[must_use]
    pub fn not_found(name: &str, lib: &str) -> Self {
        Self::id("CPF9801").text(name, 10).text(lib, 10)
    }

    /// `CPF9810`: library not found.
    #[must_use]
    pub fn no_library(lib: &str) -> Self {
        Self::id("CPF9810").text(lib, 10)
    }
}

/// The result of a provider call.
pub type ApiResult<T> = Result<T, ApiFail>;

/// An object's description.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjInfo {
    /// Name.
    pub name: String,
    /// Library.
    pub lib: String,
    /// `*PGM`, `*FILE`, ...
    pub ty: String,
    /// Extended attribute (`PF`, `RPGLE`, ...).
    pub attr: String,
    /// Text.
    pub text: String,
    /// Owner.
    pub owner: String,
    /// Creation, `CYYMMDDHHMMSS`.
    pub created: String,
    /// Last change, `CYYMMDDHHMMSS`.
    pub changed: String,
    /// Source file, library and member.
    pub source: Option<(String, String, String)>,
    /// User-defined attribute.
    pub user_attr: String,
}

/// User index attributes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdxAttr {
    /// `F` fixed, `V` variable entry length.
    pub fixed: bool,
    /// Entry length (maximum when variable).
    pub entry_len: u32,
    /// Keyed insertion.
    pub keyed: bool,
    /// Key length.
    pub key_len: u32,
    /// Immediate update.
    pub immediate: bool,
    /// Optimized for random (`0`) or sequential (`1`) access.
    pub sequential: bool,
    /// Text.
    pub text: String,
}

/// One data queue entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DtaqEntry {
    /// Key, empty on a non-keyed queue.
    pub key: Vec<u8>,
    /// Data.
    pub data: Vec<u8>,
    /// Sender identification, when kept.
    pub sender: Vec<u8>,
}

/// A data queue's attributes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DtaqInfo {
    /// Name.
    pub name: String,
    /// Library.
    pub lib: String,
    /// Maximum entry length.
    pub max_len: u32,
    /// Key length, 0 when not keyed.
    pub key_len: u32,
    /// `F`, `L` or `K`.
    pub sequence: char,
    /// Sender identification kept.
    pub sender_id: bool,
    /// Text.
    pub text: String,
    /// Entries now.
    pub entries: u32,
    /// Entries allocated.
    pub allocated: u32,
}

/// Data area types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataAreaType {
    /// `*CHAR`.
    Char,
    /// `*DEC`.
    Dec,
    /// `*LGL`.
    Lgl,
}

impl DataAreaType {
    /// `*CHAR`, `*DEC` or `*LGL`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Char => "*CHAR",
            Self::Dec => "*DEC",
            Self::Lgl => "*LGL",
        }
    }
}

/// A data area's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataArea {
    /// Type.
    pub ty: DataAreaType,
    /// Library it was found in.
    pub lib: String,
    /// Length (characters or digits).
    pub len: u32,
    /// Decimal positions.
    pub decimals: u32,
    /// The value: characters (EBCDIC), or the decimal's packed bytes.
    pub value: Vec<u8>,
}

/// System value kinds as `QWCRSVAL` types them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysValKind {
    /// Character data, `C`.
    Char,
    /// Binary data, `B`.
    Bin,
}

/// A system value or network attribute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SysVal {
    /// Kind.
    pub kind: SysValKind,
    /// The bytes: EBCDIC characters, or a big-endian `i32`.
    pub data: Vec<u8>,
}

/// The clock: Unix seconds, local offset and the time zone's name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalNow {
    /// Seconds since 1970 UTC.
    pub unix: u64,
    /// Local offset from UTC, minutes.
    pub offset_min: i32,
    /// `*TIMZON` name.
    pub zone: String,
    /// Daylight saving time in effect.
    pub dst: bool,
}

/// A file's description.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileInfo {
    /// Name.
    pub name: String,
    /// Library.
    pub lib: String,
    /// `PF`, `LF`.
    pub attr: String,
    /// Text.
    pub text: String,
    /// Record length.
    pub rec_len: u32,
    /// Keyed access path.
    pub keyed: bool,
    /// Source file.
    pub source: bool,
    /// Based-on files of a logical file.
    pub based_on: Vec<(String, String)>,
}

/// A member's description.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemberInfo {
    /// Name.
    pub name: String,
    /// Source type.
    pub srctype: String,
    /// Created, `CYYMMDDHHMMSS`.
    pub created: String,
    /// Changed, `CYYMMDDHHMMSS`.
    pub changed: String,
    /// Text.
    pub text: String,
    /// Current records.
    pub records: u32,
    /// Deleted records.
    pub deleted: u32,
}

/// A record format.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormatInfo {
    /// Name.
    pub name: String,
    /// Record length.
    pub rec_len: u32,
    /// Text.
    pub text: String,
    /// Fields in order.
    pub fields: Vec<FieldInfo>,
}

/// A field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldInfo {
    /// Name.
    pub name: String,
    /// DDS data type: `A`, `P`, `S`, `B`, `F`, `L`, `T`, `Z`, ...
    pub ty: char,
    /// Buffer position, 1-based.
    pub pos: u32,
    /// Bytes.
    pub len: u32,
    /// Digits.
    pub digits: u32,
    /// Decimal positions.
    pub decimals: u32,
    /// Text.
    pub text: String,
    /// Column headings.
    pub headings: [String; 3],
}

/// A dependent file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DepInfo {
    /// Name.
    pub name: String,
    /// Library.
    pub lib: String,
    /// Member.
    pub member: String,
    /// `L` logical file, `C` constraint.
    pub kind: char,
}

/// Commitment control state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommitInfo {
    /// `*CHG`, `*CS`, `*ALL`.
    pub lock_level: String,
    /// Uncommitted changes exist.
    pub pending: bool,
    /// Default journal, if any.
    pub journal: Option<(String, String)>,
}

/// A user profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserInfo {
    /// Name.
    pub name: String,
    /// `*ENABLED` or `*DISABLED`.
    pub status: String,
    /// `*SECOFR`, `*USER`, ...
    pub class: String,
    /// Special authorities.
    pub special: Vec<String>,
    /// Text.
    pub text: String,
}

/// Where the APIs' objects live.
#[allow(unused_variables)]
pub trait Provider {
    /// Objects in `lib` whose name matches `name` (`*ALL`, a name or
    /// `ABC*`) and type `ty` (`*ALL` or `*PGM` ...).
    fn list_objects(&mut self, lib: &str, name: &str, ty: &str) -> Vec<ObjInfo> {
        Vec::new()
    }
    /// One object's description.
    fn object(&mut self, lib: &str, name: &str, ty: &str) -> Option<ObjInfo> {
        None
    }
    /// Change text or user attribute; the library it was found in.
    fn change_object(
        &mut self,
        lib: &str,
        name: &str,
        ty: &str,
        text: Option<String>,
        user_attr: Option<String>,
    ) -> ApiResult<String> {
        Err(ApiFail::not_found(name, lib))
    }
    /// Rename or move; the library it was found in.
    fn rename_object(
        &mut self,
        lib: &str,
        name: &str,
        ty: &str,
        new_lib: &str,
        new_name: &str,
        replace: bool,
    ) -> ApiResult<String> {
        Err(ApiFail::not_found(name, lib))
    }

    /// Create a user index.
    fn usridx_create(&mut self, lib: &str, name: &str, attr: IdxAttr) -> ApiResult<()> {
        Err(ApiFail::no_library(lib))
    }
    /// Delete a user index.
    fn usridx_delete(&mut self, lib: &str, name: &str) -> ApiResult<()> {
        Err(ApiFail::not_found(name, lib))
    }
    /// Add entries; how many went in.
    fn usridx_add(
        &mut self,
        lib: &str,
        name: &str,
        entries: Vec<Vec<u8>>,
        replace: bool,
    ) -> ApiResult<usize> {
        Err(ApiFail::not_found(name, lib))
    }
    /// Find entries by rule: 1 equal, 2 greater, 3 less, 4 not less,
    /// 5 not greater, 6 first, 7 last, 8 between `key` and `key2`;
    /// at most `max`, removed when `remove`.
    fn usridx_find(
        &mut self,
        lib: &str,
        name: &str,
        rule: u32,
        key: &[u8],
        key2: &[u8],
        max: usize,
        remove: bool,
    ) -> ApiResult<Vec<Vec<u8>>> {
        Err(ApiFail::not_found(name, lib))
    }
    /// Attributes, entry count and the library.
    fn usridx_attrs(&mut self, lib: &str, name: &str) -> ApiResult<(IdxAttr, usize, String)> {
        Err(ApiFail::not_found(name, lib))
    }

    /// Send to a data queue.
    fn dtaq_send(
        &mut self,
        lib: &str,
        name: &str,
        key: &[u8],
        data: &[u8],
        sender: &str,
    ) -> ApiResult<()> {
        Err(ApiFail::not_found(name, lib))
    }
    /// Receive from a data queue: `order` is `EQ`, `NE`, `GT`, `GE`,
    /// `LT`, `LE` or blank; `None` when nothing matches.
    fn dtaq_receive(
        &mut self,
        lib: &str,
        name: &str,
        order: &str,
        key: &[u8],
        remove: bool,
    ) -> ApiResult<Option<DtaqEntry>> {
        Err(ApiFail::not_found(name, lib))
    }
    /// Clear a data queue, or its entries matching the key.
    fn dtaq_clear(&mut self, lib: &str, name: &str, order: &str, key: &[u8]) -> ApiResult<()> {
        Err(ApiFail::not_found(name, lib))
    }
    /// Attributes.
    fn dtaq_describe(&mut self, lib: &str, name: &str) -> ApiResult<DtaqInfo> {
        Err(ApiFail::not_found(name, lib))
    }
    /// Every entry, oldest first.
    fn dtaq_entries(&mut self, lib: &str, name: &str) -> ApiResult<Vec<DtaqEntry>> {
        Err(ApiFail::not_found(name, lib))
    }

    /// A data area's value.
    fn dtaara_read(&mut self, lib: &str, name: &str) -> ApiResult<DataArea> {
        Err(ApiFail::not_found(name, lib))
    }
    /// Change part of a data area, `start` 1-based.
    fn dtaara_write(&mut self, lib: &str, name: &str, start: usize, bytes: &[u8]) -> ApiResult<()> {
        Err(ApiFail::not_found(name, lib))
    }

    /// A system value.
    fn sysval(&mut self, name: &str) -> Option<SysVal> {
        None
    }
    /// A network attribute.
    fn netattr(&mut self, name: &str) -> Option<SysVal> {
        None
    }
    /// The clock.
    fn now(&mut self) -> LocalNow {
        LocalNow::default()
    }

    /// Run a command (`QCMDEXC`).
    fn exec_command(&mut self, text: &str) -> ApiResult<()> {
        Err(ApiFail::id("CPF0006"))
    }
    /// Check a command (`QCMDCHK`).
    fn check_command(&mut self, text: &str) -> ApiResult<()> {
        Err(ApiFail::id("CPF0006"))
    }

    /// A file.
    fn file(&mut self, lib: &str, name: &str) -> Option<FileInfo> {
        None
    }
    /// Its members.
    fn members(&mut self, lib: &str, name: &str) -> Vec<MemberInfo> {
        Vec::new()
    }
    /// Its record formats with their fields.
    fn formats(&mut self, lib: &str, name: &str) -> Vec<FormatInfo> {
        Vec::new()
    }
    /// Files depending on it.
    fn dependents(&mut self, lib: &str, name: &str, member: &str) -> Vec<DepInfo> {
        Vec::new()
    }

    /// Commitment control, when active.
    fn commit_info(&mut self) -> Option<CommitInfo> {
        None
    }
    /// A user profile.
    fn user_info(&mut self, name: &str) -> Option<UserInfo> {
        None
    }
}

/// One in-memory user index.
#[derive(Clone, Debug, Default)]
pub struct MemIndex {
    /// Attributes.
    pub attr: IdxAttr,
    /// Entries in key order (whole entry when not keyed).
    pub entries: Vec<Vec<u8>>,
}

/// One in-memory data queue.
#[derive(Clone, Debug, Default)]
pub struct MemDtaq {
    /// Attributes.
    pub info: DtaqInfo,
    /// Entries, oldest first.
    pub entries: VecDeque<DtaqEntry>,
}

/// One in-memory file.
#[derive(Clone, Debug, Default)]
pub struct MemFile {
    /// Description.
    pub info: FileInfo,
    /// Members.
    pub members: Vec<MemberInfo>,
    /// Formats.
    pub formats: Vec<FormatInfo>,
    /// Dependents.
    pub dependents: Vec<DepInfo>,
}

/// Everything in memory, seeded by tests.
pub struct MemProvider {
    /// Objects.
    pub objects: Vec<ObjInfo>,
    /// User indexes by `(library, name)`.
    pub indexes: HashMap<(String, String), MemIndex>,
    /// Data queues by `(library, name)`.
    pub dtaqs: HashMap<(String, String), MemDtaq>,
    /// Data areas by `(library, name)`; `*LDA` is in `*LIBL`.
    pub dtaaras: HashMap<(String, String), DataArea>,
    /// System value overrides; the catalog's defaults otherwise.
    pub sysvals: HashMap<String, SysVal>,
    /// Network attributes.
    pub netattrs: HashMap<String, SysVal>,
    /// A fixed clock for tests; the real one when `None`.
    pub clock: Option<LocalNow>,
    /// Commands run.
    pub commands: Vec<String>,
    /// Files by `(library, name)`.
    pub files: HashMap<(String, String), MemFile>,
    /// Commitment control state.
    pub commit: Option<CommitInfo>,
    /// User profiles.
    pub users: Vec<UserInfo>,
    catalog: sysval::Catalog,
}

impl Default for MemProvider {
    fn default() -> Self {
        let mut dtaaras = HashMap::new();
        dtaaras.insert(
            ("*LIBL".to_string(), "*LDA".to_string()),
            DataArea {
                ty: DataAreaType::Char,
                lib: "*LIBL".into(),
                len: 1024,
                decimals: 0,
                value: vec![0x40; 1024],
            },
        );
        let mut netattrs = HashMap::new();
        for (k, v) in [
            ("SYSNAME", "QS400"),
            ("LCLNETID", "APPN"),
            ("LCLCPNAME", "QS400"),
        ] {
            netattrs.insert(k.to_string(), SysVal::chars(v, 8));
        }
        Self {
            objects: Vec::new(),
            indexes: HashMap::new(),
            dtaqs: HashMap::new(),
            dtaaras,
            sysvals: HashMap::new(),
            netattrs,
            clock: None,
            commands: Vec::new(),
            files: HashMap::new(),
            commit: None,
            users: vec![UserInfo {
                name: "QSYS".into(),
                status: "*ENABLED".into(),
                class: "*SECOFR".into(),
                special: vec!["*ALLOBJ".into(), "*SECADM".into()],
                text: "System profile".into(),
            }],
            catalog: sysval::Catalog::builtin().expect("the catalog parses"),
        }
    }
}

impl SysVal {
    /// A character value, blank padded to `n`.
    #[must_use]
    pub fn chars(s: &str, n: usize) -> Self {
        let mut b = ccsid::to_ebcdic(s).unwrap_or_default();
        b.resize(n, 0x40);
        Self {
            kind: SysValKind::Char,
            data: b,
        }
    }

    /// A binary value.
    #[must_use]
    pub fn bin(v: i32) -> Self {
        Self {
            kind: SysValKind::Bin,
            data: v.to_be_bytes().to_vec(),
        }
    }
}

fn matches_name(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        _ if pattern == "*ALL" => true,
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

fn key_cmp(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    a.cmp(b)
}

impl MemProvider {
    /// The system clock in UTC.
    #[must_use]
    pub fn default_now() -> LocalNow {
        LocalNow {
            unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            offset_min: 0,
            zone: "QN0000UTC".into(),
            dst: false,
        }
    }

    /// Create a data queue.
    pub fn add_dtaq(&mut self, lib: &str, name: &str, max_len: u32, key_len: u32, sequence: char) {
        self.dtaqs.insert(
            (lib.into(), name.into()),
            MemDtaq {
                info: DtaqInfo {
                    name: name.into(),
                    lib: lib.into(),
                    max_len,
                    key_len,
                    sequence,
                    sender_id: true,
                    text: String::new(),
                    entries: 0,
                    allocated: 16,
                },
                entries: VecDeque::new(),
            },
        );
    }

    /// Create a data area.
    pub fn add_dtaara(&mut self, lib: &str, name: &str, area: DataArea) {
        self.dtaaras.insert((lib.into(), name.into()), area);
    }

    fn find_key<T>(
        map: &HashMap<(String, String), T>,
        lib: &str,
        name: &str,
    ) -> Option<(String, String)> {
        if lib == "*LIBL" || lib == "*CURLIB" {
            for l in ["QSYS", "QGPL", "QTEMP", "*LIBL"] {
                let k = (l.to_string(), name.to_string());
                if map.contains_key(&k) {
                    return Some(k);
                }
            }
            map.keys().find(|(_, n)| n == name).cloned()
        } else {
            let k = (lib.to_string(), name.to_string());
            map.contains_key(&k).then_some(k)
        }
    }

    fn order_ok(order: &str, entry: &[u8], key: &[u8]) -> bool {
        let c = key_cmp(entry, key);
        match order.trim() {
            "" | "EQ" => c.is_eq(),
            "NE" => c.is_ne(),
            "GT" => c.is_gt(),
            "GE" => c.is_ge(),
            "LT" => c.is_lt(),
            "LE" => c.is_le(),
            _ => false,
        }
    }
}

impl Provider for MemProvider {
    fn list_objects(&mut self, lib: &str, name: &str, ty: &str) -> Vec<ObjInfo> {
        self.objects
            .iter()
            .filter(|o| {
                (lib == "*LIBL" || lib == "*ALL" || o.lib == lib)
                    && matches_name(name, &o.name)
                    && (ty == "*ALL" || o.ty == ty)
            })
            .cloned()
            .collect()
    }

    fn object(&mut self, lib: &str, name: &str, ty: &str) -> Option<ObjInfo> {
        self.list_objects(lib, name, ty).into_iter().next()
    }

    fn change_object(
        &mut self,
        lib: &str,
        name: &str,
        ty: &str,
        text: Option<String>,
        user_attr: Option<String>,
    ) -> ApiResult<String> {
        let o = self
            .objects
            .iter_mut()
            .find(|o| {
                (lib == "*LIBL" || o.lib == lib) && o.name == name && (ty == "*ALL" || o.ty == ty)
            })
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        if let Some(t) = text {
            o.text = t;
        }
        if let Some(a) = user_attr {
            o.user_attr = a;
        }
        Ok(o.lib.clone())
    }

    fn rename_object(
        &mut self,
        lib: &str,
        name: &str,
        ty: &str,
        new_lib: &str,
        new_name: &str,
        replace: bool,
    ) -> ApiResult<String> {
        let target_lib = if new_lib == "*SAME" || new_lib == "*LIBL" {
            None
        } else {
            Some(new_lib.to_string())
        };
        let i = self
            .objects
            .iter()
            .position(|o| {
                (lib == "*LIBL" || o.lib == lib) && o.name == name && (ty == "*ALL" || o.ty == ty)
            })
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        let found_lib = self.objects[i].lib.clone();
        let dest_lib = target_lib.unwrap_or_else(|| found_lib.clone());
        if let Some(j) = self
            .objects
            .iter()
            .position(|o| o.lib == dest_lib && o.name == new_name && o.ty == self.objects[i].ty)
        {
            if !replace {
                return Err(ApiFail::id("CPF2132")
                    .text(new_name, 10)
                    .text(&dest_lib, 10));
            }
            self.objects.remove(j);
        }
        let i = self
            .objects
            .iter()
            .position(|o| o.lib == found_lib && o.name == name)
            .expect("still there");
        self.objects[i].name = new_name.into();
        self.objects[i].lib = dest_lib;
        Ok(found_lib)
    }

    fn usridx_create(&mut self, lib: &str, name: &str, attr: IdxAttr) -> ApiResult<()> {
        let lib = if lib == "*CURLIB" || lib == "*LIBL" {
            "QGPL"
        } else {
            lib
        };
        let k = (lib.to_string(), name.to_string());
        if self.indexes.contains_key(&k) {
            return Err(ApiFail::id("CPF9870").text(name, 10).text(lib, 10));
        }
        self.indexes.insert(
            k,
            MemIndex {
                attr,
                entries: Vec::new(),
            },
        );
        Ok(())
    }

    fn usridx_delete(&mut self, lib: &str, name: &str) -> ApiResult<()> {
        let k = Self::find_key(&self.indexes, lib, name)
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        self.indexes.remove(&k);
        Ok(())
    }

    fn usridx_add(
        &mut self,
        lib: &str,
        name: &str,
        entries: Vec<Vec<u8>>,
        replace: bool,
    ) -> ApiResult<usize> {
        let k = Self::find_key(&self.indexes, lib, name)
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        let idx = self.indexes.get_mut(&k).expect("found");
        let kl = idx.attr.key_len as usize;
        let mut n = 0;
        for e in entries {
            let key = |x: &Vec<u8>| x.get(..kl.min(x.len())).unwrap_or(&[]).to_vec();
            let ek = key(&e);
            match idx.entries.binary_search_by(|x| key_cmp(&key(x), &ek)) {
                Ok(i) if idx.attr.keyed => {
                    if replace {
                        idx.entries[i] = e;
                        n += 1;
                    } else {
                        return Err(ApiFail::id("CPF3C52").bin4(i32::try_from(n).unwrap_or(0)));
                    }
                }
                Ok(i) | Err(i) => {
                    idx.entries.insert(i, e);
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    fn usridx_find(
        &mut self,
        lib: &str,
        name: &str,
        rule: u32,
        key: &[u8],
        key2: &[u8],
        max: usize,
        remove: bool,
    ) -> ApiResult<Vec<Vec<u8>>> {
        let k = Self::find_key(&self.indexes, lib, name)
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        let idx = self.indexes.get_mut(&k).expect("found");
        let kl = idx.attr.key_len as usize;
        let ek = |x: &Vec<u8>| x.get(..kl.min(x.len())).unwrap_or(&[]).to_vec();
        let hits: Vec<usize> = match rule {
            6 => idx
                .entries
                .iter()
                .enumerate()
                .map(|(i, _)| i)
                .take(max)
                .collect(),
            7 => idx
                .entries
                .iter()
                .enumerate()
                .map(|(i, _)| i)
                .rev()
                .take(max)
                .collect(),
            _ => idx
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    let c = key_cmp(&ek(e), key);
                    match rule {
                        1 => c.is_eq(),
                        2 => c.is_gt(),
                        3 => c.is_lt(),
                        4 => c.is_ge(),
                        5 => c.is_le(),
                        8 => c.is_ge() && key_cmp(&ek(e), key2).is_le(),
                        _ => false,
                    }
                })
                .map(|(i, _)| i)
                .take(max)
                .collect(),
        };
        let out: Vec<Vec<u8>> = hits.iter().map(|&i| idx.entries[i].clone()).collect();
        if remove {
            let mut sorted = hits;
            sorted.sort_unstable();
            for i in sorted.into_iter().rev() {
                idx.entries.remove(i);
            }
        }
        Ok(out)
    }

    fn usridx_attrs(&mut self, lib: &str, name: &str) -> ApiResult<(IdxAttr, usize, String)> {
        let k = Self::find_key(&self.indexes, lib, name)
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        let idx = &self.indexes[&k];
        Ok((idx.attr.clone(), idx.entries.len(), k.0))
    }

    fn dtaq_send(
        &mut self,
        lib: &str,
        name: &str,
        key: &[u8],
        data: &[u8],
        sender: &str,
    ) -> ApiResult<()> {
        let k =
            Self::find_key(&self.dtaqs, lib, name).ok_or_else(|| ApiFail::not_found(name, lib))?;
        let q = self.dtaqs.get_mut(&k).expect("found");
        if data.len() > q.info.max_len as usize {
            return Err(ApiFail::id("CPF9506").text(name, 10).text(&k.0, 10));
        }
        let e = DtaqEntry {
            key: key.to_vec(),
            data: data.to_vec(),
            sender: ccsid::to_ebcdic(sender).unwrap_or_default(),
        };
        if q.info.sequence == 'L' {
            q.entries.push_front(e);
        } else {
            q.entries.push_back(e);
        }
        q.info.entries = u32::try_from(q.entries.len()).unwrap_or(u32::MAX);
        Ok(())
    }

    fn dtaq_receive(
        &mut self,
        lib: &str,
        name: &str,
        order: &str,
        key: &[u8],
        remove: bool,
    ) -> ApiResult<Option<DtaqEntry>> {
        let k =
            Self::find_key(&self.dtaqs, lib, name).ok_or_else(|| ApiFail::not_found(name, lib))?;
        let q = self.dtaqs.get_mut(&k).expect("found");
        let i = if q.info.sequence == 'K' {
            q.entries
                .iter()
                .position(|e| Self::order_ok(order, &e.key, key))
        } else {
            (!q.entries.is_empty()).then_some(0)
        };
        let Some(i) = i else {
            return Ok(None);
        };
        let e = if remove {
            q.entries.remove(i).expect("present")
        } else {
            q.entries[i].clone()
        };
        q.info.entries = u32::try_from(q.entries.len()).unwrap_or(u32::MAX);
        Ok(Some(e))
    }

    fn dtaq_clear(&mut self, lib: &str, name: &str, order: &str, key: &[u8]) -> ApiResult<()> {
        let k =
            Self::find_key(&self.dtaqs, lib, name).ok_or_else(|| ApiFail::not_found(name, lib))?;
        let q = self.dtaqs.get_mut(&k).expect("found");
        if q.info.sequence == 'K' && !order.trim().is_empty() {
            q.entries.retain(|e| !Self::order_ok(order, &e.key, key));
        } else {
            q.entries.clear();
        }
        q.info.entries = u32::try_from(q.entries.len()).unwrap_or(u32::MAX);
        Ok(())
    }

    fn dtaq_describe(&mut self, lib: &str, name: &str) -> ApiResult<DtaqInfo> {
        let k =
            Self::find_key(&self.dtaqs, lib, name).ok_or_else(|| ApiFail::not_found(name, lib))?;
        Ok(self.dtaqs[&k].info.clone())
    }

    fn dtaq_entries(&mut self, lib: &str, name: &str) -> ApiResult<Vec<DtaqEntry>> {
        let k =
            Self::find_key(&self.dtaqs, lib, name).ok_or_else(|| ApiFail::not_found(name, lib))?;
        Ok(self.dtaqs[&k].entries.iter().cloned().collect())
    }

    fn dtaara_read(&mut self, lib: &str, name: &str) -> ApiResult<DataArea> {
        let k = Self::find_key(&self.dtaaras, lib, name)
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        let mut a = self.dtaaras[&k].clone();
        a.lib.clone_from(&k.0);
        Ok(a)
    }

    fn dtaara_write(&mut self, lib: &str, name: &str, start: usize, bytes: &[u8]) -> ApiResult<()> {
        let k = Self::find_key(&self.dtaaras, lib, name)
            .ok_or_else(|| ApiFail::not_found(name, lib))?;
        let a = self.dtaaras.get_mut(&k).expect("found");
        let end = start.saturating_sub(1) + bytes.len();
        if start == 0 || end > a.value.len() {
            return Err(ApiFail::id("CPF1089").text(name, 10));
        }
        a.value[start - 1..end].copy_from_slice(bytes);
        Ok(())
    }

    fn sysval(&mut self, name: &str) -> Option<SysVal> {
        if let Some(v) = self.sysvals.get(name) {
            return Some(v.clone());
        }
        let e = self.catalog.get(name)?;
        Some(match e.default_value() {
            sysval::Value::Dec(n) => SysVal::bin(i32::try_from(n).unwrap_or(0)),
            sysval::Value::Char(s) => SysVal::chars(&s, s.len().max(1)),
            sysval::Value::List(v) => SysVal::chars(&v.join(" "), v.join(" ").len().max(1)),
        })
    }

    fn netattr(&mut self, name: &str) -> Option<SysVal> {
        self.netattrs.get(name).cloned()
    }

    fn now(&mut self) -> LocalNow {
        self.clock.clone().unwrap_or_else(Self::default_now)
    }

    fn exec_command(&mut self, text: &str) -> ApiResult<()> {
        self.commands.push(text.to_string());
        Ok(())
    }

    fn check_command(&mut self, text: &str) -> ApiResult<()> {
        if text.trim().is_empty() {
            return Err(ApiFail::id("CPF0006"));
        }
        Ok(())
    }

    fn file(&mut self, lib: &str, name: &str) -> Option<FileInfo> {
        let k = Self::find_key(&self.files, lib, name)?;
        let mut f = self.files[&k].info.clone();
        f.lib.clone_from(&k.0);
        Some(f)
    }

    fn members(&mut self, lib: &str, name: &str) -> Vec<MemberInfo> {
        Self::find_key(&self.files, lib, name)
            .map(|k| self.files[&k].members.clone())
            .unwrap_or_default()
    }

    fn formats(&mut self, lib: &str, name: &str) -> Vec<FormatInfo> {
        Self::find_key(&self.files, lib, name)
            .map(|k| self.files[&k].formats.clone())
            .unwrap_or_default()
    }

    fn dependents(&mut self, lib: &str, name: &str, member: &str) -> Vec<DepInfo> {
        Self::find_key(&self.files, lib, name)
            .map(|k| {
                self.files[&k]
                    .dependents
                    .iter()
                    .filter(|d| member == "*ALL" || member == "*FIRST" || d.member == member)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn commit_info(&mut self) -> Option<CommitInfo> {
        self.commit.clone()
    }

    fn user_info(&mut self, name: &str) -> Option<UserInfo> {
        self.users.iter().find(|u| u.name == name).cloned()
    }
}
