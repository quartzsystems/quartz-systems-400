// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IBM i `errno` values, exactly as `QSYSINC/SYS(ERRNO)` numbers them,
//! the Linux numbers they map from at the system call boundary
//! ([`linux`]), the message identifiers IBM pairs with them and the
//! texts `strerror` returns (Quartz-written).

pub mod linux;
mod msg;

macro_rules! errnos {
    ($($name:ident = $code:literal, $text:literal;)*) => {
        /// An IBM i `errno` value.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(i32)]
        #[allow(clippy::upper_case_acronyms)]
        pub enum Errno {
            $(
                #[doc = $text]
                $name = $code,
            )*
        }

        impl Errno {
            /// Every value, in header order.
            pub const ALL: &'static [Errno] = &[$(Errno::$name),*];

            /// The value from its number.
            #[must_use]
            pub fn from_code(code: i32) -> Option<Self> {
                match code {
                    $($code => Some(Self::$name),)*
                    _ => None,
                }
            }

            /// The macro name (`EACCES`).
            #[must_use]
            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$name => stringify!($name),)*
                }
            }

            /// The `strerror` text.
            #[must_use]
            pub fn text(self) -> &'static str {
                match self {
                    $(Self::$name => $text,)*
                }
            }
        }
    };
}

errnos! {
    EDOM = 3001, "The argument is outside the function's domain.";
    ERANGE = 3002, "The result is outside the representable range.";
    ETRUNC = 3003, "The data was truncated on input, output or update.";
    ENOTOPEN = 3004, "The file is not open.";
    ENOTREAD = 3005, "The file is not open for reading.";
    EIO = 3006, "An input or output error happened.";
    ENODEV = 3007, "There is no such device.";
    ERECIO = 3008, "Character input is not possible on a record file.";
    ENOTWRITE = 3009, "The file is not open for writing.";
    ESTDIN = 3010, "Standard input could not be opened.";
    ESTDOUT = 3011, "Standard output could not be opened.";
    ESTDERR = 3012, "Standard error could not be opened.";
    EBADSEEK = 3013, "The seek offset is not valid.";
    EBADNAME = 3014, "The object name is not valid.";
    EBADMODE = 3015, "The file mode is not valid.";
    EBADPOS = 3017, "The position is not valid.";
    ENOPOS = 3018, "There is no record at this position.";
    ENUMMBRS = 3019, "The file has too many members for this operation.";
    ENUMRECS = 3020, "The file has too many records for this operation.";
    EINVAL = 3021, "A value passed to the function is not valid.";
    EBADFUNC = 3022, "The function is not valid for this file.";
    ENOENT = 3025, "No object has this name.";
    ENOREC = 3026, "No record was found.";
    EPERM = 3027, "The operation is not permitted.";
    EBADDATA = 3028, "The message data is not valid.";
    EBUSY = 3029, "The object is in use.";
    EBADOPT = 3040, "An option is not valid.";
    ENOTUPD = 3041, "The file is not open for update.";
    ENOTDLT = 3042, "The file is not open for delete.";
    EPAD = 3043, "The data was padded to the record length.";
    EBADKEYLN = 3044, "The key length is not valid.";
    EPUTANDGET = 3080, "A read followed a write without a positioning call.";
    EGETANDPUT = 3081, "A write followed a read without a positioning call.";
    EIOERROR = 3101, "An unrecoverable input or output error happened.";
    EIORECERR = 3102, "A recoverable input or output error happened.";
    EACCES = 3401, "Permission denied.";
    ENOTDIR = 3403, "A component of the path is not a directory.";
    ENOSPC = 3404, "There is no space left.";
    EXDEV = 3405, "The link would cross file systems.";
    EAGAIN = 3406, "The operation would have to wait; try again.";
    EINTR = 3407, "The call was interrupted.";
    EFAULT = 3408, "An address passed to the function is not valid.";
    ETIME = 3409, "The operation timed out.";
    ENXIO = 3415, "There is no such device or address.";
    ECLOSED = 3417, "The socket is closed.";
    EAPAR = 3418, "The other end is gone.";
    ERECURSE = 3419, "A recursive call is not allowed.";
    EADDRINUSE = 3420, "The address is already in use.";
    EADDRNOTAVAIL = 3421, "The address is not available.";
    EAFNOSUPPORT = 3422, "The address family is not supported.";
    EALREADY = 3423, "The operation is already in progress.";
    ECONNABORTED = 3424, "The connection was ended by the local side.";
    ECONNREFUSED = 3425, "The connection was refused.";
    ECONNRESET = 3426, "The connection was reset by the other side.";
    EDESTADDRREQ = 3427, "The operation needs a destination address.";
    EHOSTDOWN = 3428, "The host is down.";
    EHOSTUNREACH = 3429, "There is no route to the host.";
    EINPROGRESS = 3430, "The operation is in progress.";
    EISCONN = 3431, "The socket is already connected.";
    EMSGSIZE = 3432, "The message is too long.";
    ENETDOWN = 3433, "The network is down.";
    ENETRESET = 3434, "The network dropped the connection.";
    ENETUNREACH = 3435, "The network cannot be reached.";
    ENOBUFS = 3436, "There is no buffer space.";
    ENOPROTOOPT = 3437, "The protocol option is not available.";
    ENOTCONN = 3438, "The socket is not connected.";
    ENOTSOCK = 3439, "The descriptor is not a socket.";
    ENOTSUP = 3440, "The operation is not supported.";
    EPFNOSUPPORT = 3441, "The protocol family is not supported.";
    EPROTONOSUPPORT = 3442, "The protocol is not supported.";
    EPROTOTYPE = 3443, "The protocol does not fit the socket type.";
    ERCVDERR = 3444, "An error was received from the other side.";
    ESHUTDOWN = 3445, "The socket has been shut down for sending.";
    ESOCKTNOSUPPORT = 3446, "The socket type is not supported.";
    ETIMEDOUT = 3447, "The connection timed out.";
    EUNATCH = 3448, "The protocol driver is not attached.";
    EBADF = 3450, "The descriptor is not valid.";
    EMFILE = 3452, "The job has too many open files.";
    ENFILE = 3453, "The system has too many open files.";
    EPIPE = 3455, "The pipe or socket has no reader.";
    ECANCELED = 3456, "The operation was canceled.";
    EEXIST = 3457, "The object already exists.";
    EDEADLK = 3459, "Waiting would deadlock.";
    ENOMEM = 3460, "There is not enough storage.";
    EOWNERTERM = 3462, "The owner of the resource has ended.";
    EDESTROYED = 3463, "The object was destroyed.";
    ETERM = 3464, "The operation was ended.";
    EMLINK = 3468, "The object has too many links.";
    ESPIPE = 3469, "The descriptor does not allow seeking.";
    ENOSYS = 3470, "The function is not implemented.";
    EISDIR = 3471, "The object is a directory.";
    EROFS = 3472, "The file system is read-only.";
    EUNKNOWN = 3474, "An unknown system state was found.";
    EITERBAD = 3475, "The iterator is not valid.";
    EDAMAGE = 3484, "The object is damaged.";
    ELOOP = 3485, "Resolving the path met too many symbolic links.";
    ENAMETOOLONG = 3486, "A name or the path is too long.";
    ENOLCK = 3487, "No locks are available.";
    ENOTEMPTY = 3488, "The directory is not empty.";
    ENOSYSRSC = 3489, "A system resource is not available.";
    ECONVERT = 3490, "The data could not be converted.";
    E2BIG = 3491, "The argument list is too long.";
    EILSEQ = 3492, "The data holds a character sequence that is not valid.";
    ESOFTDAMAGE = 3497, "The object has soft damage.";
    ENOTENROLL = 3498, "The user is not enrolled in the system directory.";
    EOFFLINE = 3499, "The object is suspended or offline.";
    EROOBJ = 3500, "The object is read-only.";
    ELOCKED = 3506, "The area is locked.";
    EFBIG = 3507, "The object is too large.";
    EIDRM = 3509, "The identifier was removed.";
    ENOMSG = 3510, "No message of the wanted type is available.";
    EFILECVT = 3511, "The file ID conversion of a directory failed.";
    EBADFID = 3512, "The file ID is not valid.";
    ESTALE = 3513, "The file handle is stale.";
    ESRCH = 3515, "There is no such process.";
    ENOTSIGINIT = 3516, "The process is not set up for signals.";
    ECHILD = 3517, "There is no child process.";
    ETOOMANYREFS = 3523, "There are too many references.";
    ENOTSAFE = 3524, "The function is not safe to call here.";
    EOVERFLOW = 3525, "The value does not fit the field.";
    EJRNDAMAGE = 3526, "The journal is damaged.";
    EJRNINACTIVE = 3527, "The journal is not active.";
    EJRNRCVSPC = 3528, "The journal receiver has no space left.";
    EJRNRMT = 3529, "The journal is remote.";
    ENEWJRNRCV = 3530, "A new journal receiver is needed.";
    ENEWJRN = 3531, "A new journal is needed.";
    EJOURNALED = 3532, "The object is already journaled.";
    EJRNENTTOOLONG = 3533, "The journal entry is too long.";
    EDATALINK = 3534, "The object is linked to a data link.";
    ENOTAVAIL = 3535, "The independent ASP is not available.";
    ENOTTY = 3536, "The operation needs a terminal.";
    EFBIG2 = 3540, "The file is too large for this open.";
    ETXTBSY = 3543, "The file is busy.";
    EASPGRPNOTSET = 3544, "The ASP group is not set for the thread.";
    ERESTART = 3545, "The call should be restarted.";
    ESCANFAILURE = 3546, "The object failed a scan.";
    EGROUPDOWN = 3572, "A member of the group is down.";
}

impl Errno {
    /// `EWOULDBLOCK`, the same value as `EAGAIN`.
    pub const EWOULDBLOCK: Self = Self::EAGAIN;
    /// `EOPNOTSUPP`, the same value as `ENOTSUP`.
    pub const EOPNOTSUPP: Self = Self::ENOTSUP;
    /// `ECANCEL`, the same value as `ECANCELED`.
    pub const ECANCEL: Self = Self::ECANCELED;

    /// The number.
    #[must_use]
    pub fn code(self) -> i32 {
        self as i32
    }

    /// The message IBM pairs with every `errno`: `CPE` and the number.
    #[must_use]
    pub fn message_id(self) -> String {
        format!("CPE{}", self.code())
    }

    /// The escape message an IFS command sends for this error (the
    /// `CPFA0xx` family); errors without one of their own are
    /// `CPFA0D4`, which carries the number.
    #[must_use]
    pub fn command_message(self) -> &'static str {
        msg::command_message(self)
    }

    /// Whether [`Errno::command_message`] takes the object's path as its
    /// first substitution value (the `Object is &1` messages).
    #[must_use]
    pub fn command_message_names_object(self) -> bool {
        msg::names_object(self)
    }
}

impl std::fmt::Display for Errno {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({}): {}", self.name(), self.code(), self.text())
    }
}

impl std::error::Error for Errno {}

/// What every IFS operation returns.
pub type Result<T> = std::result::Result<T, Errno>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_match_the_header() {
        assert_eq!(Errno::EACCES.code(), 3401);
        assert_eq!(Errno::ENOENT.code(), 3025);
        assert_eq!(Errno::EWOULDBLOCK, Errno::EAGAIN);
        assert_eq!(Errno::EOPNOTSUPP.code(), 3440);
        assert_eq!(Errno::from_code(3485), Some(Errno::ELOOP));
        assert_eq!(Errno::from_code(1), None);
        assert_eq!(Errno::ENOTEMPTY.message_id(), "CPE3488");
        assert_eq!(Errno::ALL.len(), 137);
    }

    /// Every `#define E... number` in the captured header is here with
    /// the same number, and nothing else is.
    #[test]
    fn header_capture_agrees() {
        let hdr = include_str!("../../../../tools/conformance/oracle/qsysinc/SYS/ERRNO.txt");
        let mut seen = 0;
        for line in hdr.lines() {
            let mut w = line.split_whitespace();
            if w.next() != Some("#define") {
                continue;
            }
            let (Some(name), Some(val)) = (w.next(), w.next()) else {
                continue;
            };
            let Ok(n) = val.parse::<i32>() else { continue };
            let e = Errno::from_code(n).unwrap_or_else(|| panic!("{name} {n} missing"));
            let alias = matches!(name, "EWOULDBLOCK" | "EOPNOTSUPP" | "ECANCEL");
            assert!(alias || e.name() == name, "{name} is {}", e.name());
            seen += usize::from(!alias);
        }
        assert_eq!(seen, Errno::ALL.len());
    }
}
