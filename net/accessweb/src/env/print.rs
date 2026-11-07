// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Printer output and printers (handoff step 15): the spooled files the
//! signed-on user may see, their pages as text and PDF, the actions on
//! them (`HLDSPLF`, `RLSSPLF`, `DLTSPLF`, `CHGSPLFA OUTQ()`), the
//! printers. Every call runs with the user's authority; a refusal is
//! the system's message.

use qsweb::msg::Message;

/// A spooled file, as Printer Output and its properties show it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Splf {
    /// The job that made it, `123402/CWELLMAN/INVBATCH`.
    pub job: String,
    /// The file name.
    pub file: String,
    /// The file number in its job.
    pub number: u32,
    /// The user who owns it.
    pub user: String,
    /// User data.
    pub user_data: String,
    /// `RDY`, `HLD`, `SAV`, `WTR`, `MSGW`, `OPN`, ...
    pub status: String,
    /// Pages.
    pub pages: u32,
    /// The page a writer is on, 0 when none.
    pub current_page: u32,
    /// Copies.
    pub copies: u32,
    /// Form type.
    pub form_type: String,
    /// `QGPL/QPRINT`.
    pub outq: String,
    /// Created, seconds since 1970.
    pub created: u64,
    /// Output priority.
    pub priority: u8,
    /// `*SCS`, `*AFPDS`, `*USERASCII`, ...
    pub device_type: String,
    /// Lines a page.
    pub page_len: u32,
    /// Columns a line.
    pub page_width: u32,
    /// Lines per inch.
    pub lpi: u32,
    /// Characters per inch.
    pub cpi: u32,
    /// Overflow line.
    pub overflow: u32,
    /// `HOLD` of the printer file.
    pub hold: bool,
    /// `SAVE` of the printer file.
    pub save: bool,
    /// Record length.
    pub record_len: u32,
    /// Records in all.
    pub records: u32,
    /// The printer device, `*OUTQ` for the queue's.
    pub printer: String,
    /// The writer printing it, empty when none.
    pub writer: String,
}

impl Splf {
    /// Its key in URLs and forms: `123402-CWELLMAN-INVBATCH-INVRPT-402`
    /// (system names hold no `-`).
    pub fn id(&self) -> String {
        let mut j = self.job.split('/');
        let (n, u, name) = (j.next().unwrap_or(""), j.next().unwrap_or(""), j.next().unwrap_or(""));
        format!("{n}-{u}-{name}-{}-{}", self.file, self.number)
    }

    /// The command parameters naming it.
    pub fn parms(&self) -> String {
        format!("FILE({}) JOB({}) SPLNBR({})", self.file, self.job, self.number)
    }
}

/// What to do to a spooled file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// `HLDSPLF OPTION(*IMMED)`, or `*PAGEEND` at the end of the page.
    Hold {
        /// At the end of the page being printed.
        page_end: bool,
    },
    /// `RLSSPLF`.
    Release,
    /// `DLTSPLF`.
    Delete,
    /// `CHGSPLFA OUTQ()`, to the top of the queue when `top`.
    Move {
        /// `LIB/QUEUE`.
        outq: String,
        /// Ahead of the files waiting there.
        top: bool,
    },
}

/// A printer, as Printers lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Printer {
    /// The device or writer name.
    pub name: String,
    /// Whether its writer is started.
    pub started: bool,
    /// Its output queue, `QGPL/QPRINT`.
    pub outq: String,
    /// Where it is.
    pub location: String,
    /// What kind of printer it is.
    pub kind: String,
}

/// Printer output and printers as the signed-on user sees them.
pub trait Print: Send + Sync {
    /// The spooled files of `owner` (`None`: every user's the user may
    /// see), newest first.
    fn spooled(&self, user: &str, owner: Option<&str>) -> Result<Vec<Splf>, Message>;
    /// The users the User filter offers besides `*CURRENT` and `*ALL`.
    fn users(&self, user: &str) -> Vec<String>;
    /// The output queues a file may be moved to.
    fn queues(&self, user: &str) -> Vec<String>;
    /// One spooled file by its [`Splf::id`].
    fn find(&self, user: &str, id: &str) -> Result<Splf, Message>;
    /// Its pages as text.
    fn text(&self, user: &str, f: &Splf) -> Result<Vec<String>, Message>;
    /// It as a PDF.
    fn pdf(&self, user: &str, f: &Splf) -> Result<Vec<u8>, Message>;
    /// Hold, release, delete or move it, as the user.
    fn act(&self, user: &str, f: &Splf, act: &Act) -> Result<(), Message>;
    /// The printers.
    fn printers(&self, user: &str) -> Vec<Printer>;
}
