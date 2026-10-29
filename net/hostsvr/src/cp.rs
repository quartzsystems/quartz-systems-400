// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Code points (the CP of an LL/CP parameter), per server. Each module
//! lists its points in `ALL` so `docs/HOSTSVR.md` can be checked
//! against the code.

/// The sign-on family: the sign-on server and the seed exchange and
/// start-server requests every server shares.
pub mod signon {
    crate::consts! {
        /// Client or server version (u32).
        VERSION = 0x1101,
        /// Client or server datastream level (u16).
        LEVEL = 0x1102,
        /// Client or server seed (8 bytes).
        SEED = 0x1103,
        /// User ID: 10 EBCDIC bytes in requests, CCSID (u32) and name in replies.
        USER_ID = 0x1104,
        /// Password substitute (8, 20 or 64 bytes).
        PASSWORD = 0x1105,
        /// Current sign-on date (sign-on information reply).
        CURRENT_SIGNON_DATE = 0x1106,
        /// Last sign-on date.
        LAST_SIGNON_DATE = 0x1107,
        /// Password expiration date.
        EXPIRATION_DATE = 0x1108,
        /// Protected old password (change password).
        OLD_PASSWORD = 0x110C,
        /// Protected new password (change password).
        NEW_PASSWORD = 0x110D,
        /// Client CCSID (u32).
        CLIENT_CCSID = 0x1113,
        /// Server CCSID (u32).
        SERVER_CCSID = 0x1114,
        /// Profile token or other authentication token.
        PROFILE_TOKEN = 0x1115,
        /// Profile token type (one byte, 0xF1 single use, 0xF2 multiple, 0xF3 regenerable).
        TOKEN_TYPE = 0x1116,
        /// Profile token timeout in seconds (u32).
        TOKEN_TIMEOUT = 0x1117,
        /// Password level (one byte, QPWDLVL).
        PASSWORD_LEVEL = 0x1119,
        /// Old password length in bytes (u32, SHA forms).
        OLD_PASSWORD_LENGTH = 0x111C,
        /// New password length in bytes (u32, SHA forms).
        NEW_PASSWORD_LENGTH = 0x111D,
        /// Password CCSID (u32, 13488 for the SHA forms).
        PASSWORD_CCSID = 0x111E,
        /// Server job name (CCSID u32, then the qualified name).
        JOB_NAME = 0x111F,
        /// User identity type (one byte).
        IDENTITY_TYPE = 0x1126,
        /// User identity (an EIM or Kerberos identity).
        IDENTITY = 0x1127,
        /// Return error messages (one byte, 0x01 yes).
        RETURN_MESSAGES = 0x1128,
        /// Message count (u16).
        MESSAGE_COUNT = 0x112A,
        /// One message (the extended message layout).
        MESSAGE = 0x112B,
        /// Password expiration warning days (u32).
        WARNING_DAYS = 0x112C,
        /// Additional authentication factor support (one byte).
        AAF_INDICATOR = 0x112E,
        /// Additional authentication factor (CCSID u32, UTF-8 text).
        AAF = 0x112F,
        /// Verification ID (CCSID u32, text).
        VERIFICATION_ID = 0x1130,
        /// Client IP address (CCSID u32, text).
        CLIENT_IP = 0x1131,
    }
}

/// Remote command and distributed program call.
pub mod rmtcmd {
    crate::consts! {
        /// Command text in the job CCSID (datastream levels below 10).
        COMMAND = 0x1101,
        /// A message in the basic layout.
        MESSAGE = 0x1102,
        /// One program parameter.
        PARAMETER = 0x1103,
        /// Command text with its CCSID (u32) first.
        COMMAND_CCSID = 0x1104,
        /// A message in the extended layout (with help text).
        MESSAGE_EXTENDED = 0x1106,
    }
}

/// Central server: NLS conversion tables.
pub mod central {
    crate::consts! {
        /// The code points to convert (conversion table request).
        CODE_POINTS = 0x0004,
        /// CCSID list (NLS exchange attributes reply: count u32, CCSIDs u32).
        CCSID_LIST = 0x0008,
    }
}

/// Database server requests (`0x38xx`, the request side).
pub mod db_request {
    crate::consts! {
        /// Default library name or client CCSID (exchange attributes).
        LIBRARY_NAME = 0x3801,
        /// Language feature code (exchange attributes).
        LANGUAGE_FEATURE = 0x3802,
        /// Client functional level (exchange attributes).
        CLIENT_LEVEL = 0x3803,
        /// Package name, or NLS sort sequence (exchange attributes).
        PACKAGE_NAME = 0x3804,
        /// Translate indicator.
        TRANSLATE = 0x3805,
        /// Prepared statement name.
        STATEMENT_NAME = 0x3806,
        /// Statement text (CCSID u16, length u16, text).
        STATEMENT_TEXT = 0x3807,
        /// Prepare option.
        PREPARE_OPTION = 0x3808,
        /// Open attributes.
        OPEN_ATTRIBUTES = 0x3809,
        /// Describe option.
        DESCRIBE_OPTION = 0x380A,
        /// Cursor name.
        CURSOR_NAME = 0x380B,
        /// Blocking factor (rows per fetch).
        BLOCKING_FACTOR = 0x380C,
        /// Scrollable cursor flag.
        SCROLLABLE = 0x380D,
        /// Fetch scroll option.
        FETCH_SCROLL = 0x380E,
        /// Hold indicator.
        HOLD = 0x380F,
        /// Reuse indicator.
        REUSE = 0x3810,
        /// Parameter marker data.
        MARKER_DATA = 0x3811,
        /// Statement type.
        STATEMENT_TYPE = 0x3812,
        /// Package threshold.
        PACKAGE_THRESHOLD = 0x3813,
        /// Parameter marker block indicator.
        MARKER_BLOCK = 0x3814,
        /// Query timeout.
        QUERY_TIMEOUT = 0x3817,
        /// Extended parameter marker data.
        EXTENDED_MARKER_DATA = 0x381F,
        /// Use extended formats (exchange attributes).
        EXTENDED_FORMATS = 0x3821,
        /// LOB field threshold (exchange attributes).
        LOB_THRESHOLD = 0x3822,
        /// Data compression (exchange attributes).
        DATA_COMPRESSION = 0x3823,
        /// Autocommit (exchange attributes).
        AUTOCOMMIT = 0x3824,
        /// Extended column descriptor option.
        EXTENDED_COLUMNS = 0x3829,
        /// Result set holdability.
        HOLDABILITY = 0x3830,
        /// Extended statement text (CCSID u16, length u32, text).
        EXTENDED_STATEMENT_TEXT = 0x3831,
        /// RLE-compressed data (decompressed length u32, data).
        COMPRESSED = 0x3832,
        /// Variable field compression.
        VARIABLE_COMPRESSION = 0x3833,
        /// Buffer size.
        BUFFER_SIZE = 0x3834,
        /// Interface type (exchange attributes).
        INTERFACE_TYPE = 0x383C,
        /// Interface name (exchange attributes).
        INTERFACE_NAME = 0x383D,
        /// Interface level (exchange attributes).
        INTERFACE_LEVEL = 0x383E,
        /// Close on end of file (exchange attributes).
        CLOSE_EOF = 0x383F,
    }
}

/// Database server replies (`0x38xx`, the reply side).
pub mod db_reply {
    crate::consts! {
        /// Message ID.
        MESSAGE_ID = 0x3801,
        /// First level message text.
        FIRST_LEVEL_TEXT = 0x3802,
        /// Second level message text.
        SECOND_LEVEL_TEXT = 0x3803,
        /// Server attributes.
        SERVER_ATTRIBUTES = 0x3804,
        /// Data format (original).
        DATA_FORMAT = 0x3805,
        /// Result data (original).
        RESULT_DATA = 0x3806,
        /// SQLCA.
        SQLCA = 0x3807,
        /// Parameter marker format (original).
        MARKER_FORMAT = 0x3808,
        /// Package information.
        PACKAGE_INFO = 0x380B,
        /// Extended data format.
        EXTENDED_DATA_FORMAT = 0x380C,
        /// Extended parameter marker format.
        EXTENDED_MARKER_FORMAT = 0x380D,
        /// Extended result data.
        EXTENDED_RESULT_DATA = 0x380E,
        /// LOB locator.
        LOB_LOCATOR = 0x380F,
        /// LOB data.
        LOB_DATA = 0x3810,
        /// Extended column descriptors.
        EXTENDED_COLUMNS = 0x3811,
        /// Super extended data format.
        SUPER_DATA_FORMAT = 0x3812,
        /// Super extended parameter marker format.
        SUPER_MARKER_FORMAT = 0x3813,
        /// Cursor attributes.
        CURSOR_ATTRIBUTES = 0x3814,
        /// RLE-compressed reply data.
        COMPRESSED = 0x3832,
        /// Field name, inside a super extended format's variable information.
        FIELD_NAME = 0x3840,
    }
}

/// Data queue server.
pub mod dtaq {
    crate::consts! {
        /// Entry data.
        ENTRY = 0x5001,
        /// Key.
        KEY = 0x5002,
    }
}

/// File server.
pub mod file {
    crate::consts! {
        /// Path or file name.
        NAME = 0x0002,
        /// Data read (read reply).
        READ_DATA = 0x0005,
        /// Data written (write request).
        WRITE_DATA = 0x0020,
    }
}
