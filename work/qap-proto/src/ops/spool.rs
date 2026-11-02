// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Spool` (version 3): spooled files and output queues. Flags are
//! `bool`s: one byte, 0 or 1, any other value does not decode.

use ipc::{wire_enum, wire_struct};

use crate::types::{JobId, ObjectName};

/// A spooled file: its job, its name and its number in the job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpooledFileId {
    /// The job that made it.
    pub job: JobId,
    /// The file's name (`QSYSPRT`).
    pub file: String,
    /// Its number in the job.
    pub number: u32,
}

wire_struct!(SpooledFileId { job, file, number });

/// A spooled file's status.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SpoolStatus {
    /// Waiting to print.
    #[default]
    Ready,
    /// Held.
    Held,
    /// Still open: its program is writing it.
    Open,
    /// Closed, held back until its job ends.
    Closed,
    /// Being written.
    Writing,
    /// Being printed.
    Printing,
    /// Its writer waits for a reply to a message.
    Message,
    /// Waiting to be handed to a writer.
    Pending,
    /// Printed and kept.
    Saved,
}

wire_enum!(SpoolStatus {
    0 => Ready,
    1 => Held,
    2 => Open,
    3 => Closed,
    4 => Writing,
    5 => Printing,
    6 => Message,
    7 => Pending,
    8 => Saved,
});

impl SpoolStatus {
    /// Every status, in value order.
    pub const ALL: [SpoolStatus; 9] = [
        SpoolStatus::Ready,
        SpoolStatus::Held,
        SpoolStatus::Open,
        SpoolStatus::Closed,
        SpoolStatus::Writing,
        SpoolStatus::Printing,
        SpoolStatus::Message,
        SpoolStatus::Pending,
        SpoolStatus::Saved,
    ];
}

/// The kind of data a spooled file holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeviceType {
    /// SCS.
    #[default]
    Scs,
    /// Advanced function printing.
    Afp,
    /// ASCII for the printer.
    Ascii,
    /// Line data.
    Line,
}

wire_enum!(DeviceType {
    0 => Scs,
    1 => Afp,
    2 => Ascii,
    3 => Line,
});

/// A spooled file as lists show it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpooledFile {
    /// Which file.
    pub id: SpooledFileId,
    /// Its status.
    pub status: SpoolStatus,
    /// The output queue it is on.
    pub outq: ObjectName,
    /// Its owner.
    pub user: String,
    /// `USRDTA`.
    pub user_data: String,
    /// `FORMTYPE`.
    pub form_type: String,
    /// Its data.
    pub device_type: DeviceType,
    /// Pages.
    pub pages: u32,
    /// Copies.
    pub copies: u16,
    /// Its data's size, bytes.
    pub size: u64,
    /// Made, milliseconds since 1970-01-01 UTC.
    pub created: i64,
}

wire_struct!(SpooledFile {
    id,
    status,
    outq,
    user,
    user_data,
    form_type,
    device_type,
    pages,
    copies,
    size,
    created
});

/// What `ReadSpooledFile` makes of a file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReadFormat {
    /// The data stream as spooled.
    #[default]
    Scs,
    /// UTF-8, one line per printed line, a form feed between pages.
    Text,
    /// PDF.
    Pdf,
}

wire_enum!(ReadFormat {
    0 => Scs,
    1 => Text,
    2 => Pdf,
});

/// `ListSpooledFiles`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListSpooledFiles {
    /// `*CURRENT`, `*ALL` or a profile.
    pub user: String,
    /// Only this queue's.
    pub outq: Option<ObjectName>,
    /// Only files in this status.
    pub status: Option<SpoolStatus>,
    /// Only this form type.
    pub form_type: Option<String>,
    /// Only this user data.
    pub user_data: Option<String>,
    /// The first entry, 0-based.
    pub start: u32,
    /// At most this many; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListSpooledFiles {
    user,
    outq,
    status,
    form_type,
    user_data,
    start,
    max
});

/// `ListSpooledFiles`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpooledFiles {
    /// Files matching.
    pub total: u32,
    /// Those from `start`.
    pub files: Vec<SpooledFile>,
}

wire_struct!(SpooledFiles { total, files });

/// `ReadSpooledFile`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadSpooledFile {
    /// Which file.
    pub id: SpooledFileId,
    /// What to make of it.
    pub format: ReadFormat,
    /// The first page, 1-based.
    pub first_page: u32,
    /// The last page; 0 for the last.
    pub last_page: u32,
}

wire_struct!(ReadSpooledFile {
    id,
    format,
    first_page,
    last_page
});

/// `ReadSpooledFile`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpooledData {
    /// The format made.
    pub format: ReadFormat,
    /// Pages in the whole file.
    pub pages: u32,
    /// The pages asked for.
    pub data: Vec<u8>,
}

wire_struct!(SpooledData { format, pages, data });

/// `HoldSpooledFile`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HoldSpooledFile {
    /// Which file.
    pub id: SpooledFileId,
    /// Now, or (false) at the end of the page.
    pub immediately: bool,
}

wire_struct!(HoldSpooledFile { id, immediately });

/// `ReleaseSpooledFile`'s and `DeleteSpooledFile`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpooledFileRef {
    /// Which file.
    pub id: SpooledFileId,
}

wire_struct!(SpooledFileRef { id });

/// `MoveSpooledFile`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MoveSpooledFile {
    /// Which file.
    pub id: SpooledFileId,
    /// To this queue.
    pub outq: ObjectName,
    /// To the top of it.
    pub first: bool,
}

wire_struct!(MoveSpooledFile { id, outq, first });

/// `ListOutputQueues`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListOutputQueues {
    /// `*ALL`, `*LIBL` or a library.
    pub library: String,
    /// The first entry, 0-based.
    pub start: u32,
    /// At most this many; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListOutputQueues { library, start, max });

/// An output queue as `ListOutputQueues` shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputQueue {
    /// The queue.
    pub outq: ObjectName,
    /// Its text.
    pub text: String,
    /// Files on it.
    pub files: u32,
    /// The writer attached; empty when none.
    pub writer: String,
    /// Held.
    pub held: bool,
}

wire_struct!(OutputQueue {
    outq,
    text,
    files,
    writer,
    held
});

/// `ListOutputQueues`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputQueues {
    /// Queues matching.
    pub total: u32,
    /// Those from `start`.
    pub queues: Vec<OutputQueue>,
}

wire_struct!(OutputQueues { total, queues });
