// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `CPFA0xx` escape messages IFS commands send for an `errno`. The
//! identifiers were checked against `QCPFMSG` on pub400
//! (`tools/conformance/oracle/ifs/qcpfmsg-ifs.txt`); pairs IBM does not
//! document are in `docs/NET_OPEN_QUESTIONS.md`.

use super::Errno;

/// The paired message, or `CPFA0D4` (file system error, with the number).
pub fn command_message(e: Errno) -> &'static str {
    use Errno as E;
    match e {
        E::EACCES | E::EPERM => "CPFA09C",
        E::EBUSY | E::ETXTBSY | E::ELOCKED => "CPFA09E",
        E::EDAMAGE | E::ESOFTDAMAGE => "CPFA09F",
        E::EEXIST => "CPFA0A0",
        E::EIO | E::EIOERROR | E::EIORECERR => "CPFA0A1",
        E::EINVAL | E::EFAULT | E::EBADNAME => "CPFA0A2",
        E::ELOOP => "CPFA0A3",
        E::EMFILE => "CPFA0A4",
        E::ENFILE => "CPFA0A5",
        E::EMLINK => "CPFA0A6",
        E::ENAMETOOLONG => "CPFA0A7",
        E::ENOENT => "CPFA0A9",
        E::ENOSPC | E::ENOMEM => "CPFA0AA",
        E::ENOTEMPTY => "CPFA0AC",
        E::ENOTSUP | E::ENOSYS => "CPFA0AD",
        E::EXDEV => "CPFA0B0",
        E::EROFS | E::EROOBJ => "CPFA0B1",
        E::ECONVERT | E::EILSEQ => "CPFA0D0",
        E::EISDIR => "CPFA0DA",
        E::EINTR => "CPFA0DD",
        E::EBADFID => "CPFA0E0",
        E::ESCANFAILURE => "CPFA0E8",
        _ => "CPFA0D4",
    }
}

/// Whether the message's first value is the object.
pub fn names_object(e: Errno) -> bool {
    matches!(
        command_message(e),
        "CPFA09C"
            | "CPFA09E"
            | "CPFA09F"
            | "CPFA0A0"
            | "CPFA0A9"
            | "CPFA0AC"
            | "CPFA0DA"
            | "CPFA0E8"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every identifier used exists in the capture of `QCPFMSG`.
    #[test]
    fn identifiers_exist_on_the_oracle() {
        let cap = include_str!("../../../../tools/conformance/oracle/ifs/qcpfmsg-ifs.txt");
        let ids: Vec<&str> = cap.lines().filter_map(|l| l.split('|').next()).collect();
        for e in Errno::ALL {
            let id = command_message(*e);
            assert!(ids.contains(&id), "{id}");
            assert!(ids.contains(&e.message_id().as_str()), "{}", e.message_id());
        }
        assert_eq!(command_message(Errno::ENOTSUP), "CPFA0AD");
        assert_eq!(command_message(Errno::ENOENT), "CPFA0A9");
        assert!(names_object(Errno::ENOENT));
        assert!(!names_object(Errno::ENOTSUP));
    }
}
