// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a sink and a transform know about the file they print: the
//! spooled file attributes the writer carries over from `*SPLF`.

/// `DUPLEX`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Duplex {
    /// `*NO`: one side.
    #[default]
    No,
    /// `*YES`: both sides, bound on the long edge.
    Yes,
    /// `*TUMBLE`: both sides, bound on the short edge.
    Tumble,
}

impl Duplex {
    /// From the `*SPLF` code: 0 `*NO`, 1 `*YES`, 2 `*TUMBLE`.
    #[must_use]
    pub fn from_code(c: u8) -> Self {
        match c {
            1 => Self::Yes,
            2 => Self::Tumble,
            _ => Self::No,
        }
    }

    /// The special value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::No => "*NO",
            Self::Yes => "*YES",
            Self::Tumble => "*TUMBLE",
        }
    }

    /// From the special value.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "*NO" => Self::No,
            "*YES" => Self::Yes,
            "*TUMBLE" => Self::Tumble,
            _ => return None,
        })
    }
}

/// A spooled file as the printer side sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrintJob {
    /// The spooled file name.
    pub file: String,
    /// Its number in the job.
    pub number: u32,
    /// The job: name, user, number.
    pub job: (String, String, u32),
    /// The system that spooled it.
    pub system: String,
    /// `USRDTA`.
    pub usrdta: String,
    /// `FORMTYPE`.
    pub formtype: String,
    /// `COPIES`.
    pub copies: u16,
    /// Pages in the file.
    pub pages: u32,
    /// `PAGESIZE` length, lines.
    pub page_len: u16,
    /// `PAGESIZE` width, positions.
    pub page_width: u16,
    /// `LPI`.
    pub lpi: u8,
    /// `CPI`.
    pub cpi: u8,
    /// `OVRFLW`.
    pub ovrflw: u16,
    /// `DRAWER`.
    pub drawer: u8,
    /// `DUPLEX`.
    pub duplex: Duplex,
    /// `OUTPTY`.
    pub outpty: u8,
    /// `FONT` identifier, `*CPI` when blank.
    pub font: String,
    /// `CHRID`: `*DEVD`, `*SYSVAL` or `gcgid cpgid`.
    pub chrid: String,
    /// `PRTQLTY`: `*STD`, `*DRAFT`, `*NLQ`, `*DEVD`.
    pub prtqlty: String,
    /// `ALIGN`.
    pub align: bool,
    /// `DEVTYPE` of the data: `*SCS`.
    pub devtype: String,
    /// The printer file it came from, `lib/name`.
    pub prtf: String,
    /// `USRDFNDTA`.
    pub usrdfndta: String,
    /// `SAVE`.
    pub save: bool,
}

impl Default for PrintJob {
    fn default() -> Self {
        Self {
            file: "QSYSPRT".into(),
            number: 1,
            job: ("QPRTJOB".into(), "QUSER".into(), 1),
            system: "QS400".into(),
            usrdta: String::new(),
            formtype: "*STD".into(),
            copies: 1,
            pages: 1,
            page_len: 66,
            page_width: 132,
            lpi: 6,
            cpi: 10,
            ovrflw: 60,
            drawer: 1,
            duplex: Duplex::No,
            outpty: 5,
            font: "*CPI".into(),
            chrid: "*DEVD".into(),
            prtqlty: "*STD".into(),
            align: false,
            devtype: "*SCS".into(),
            prtf: "QSYS/QSYSPRT".into(),
            usrdfndta: String::new(),
            save: false,
        }
    }
}

impl PrintJob {
    /// `number/user/name`, the job as commands show it.
    #[must_use]
    pub fn job_text(&self) -> String {
        format!("{:06}/{}/{}", self.job.2, self.job.1, self.job.0)
    }

    /// A short name for the file at the printer: `FILE.NNNNNN.NN`.
    #[must_use]
    pub fn title(&self) -> String {
        format!("{}.{:06}.{}", self.file, self.job.2, self.number)
    }

    /// The page width in inches.
    #[must_use]
    pub fn width_inches(&self) -> f64 {
        f64::from(self.page_width.max(1)) / f64::from(self.cpi.max(1))
    }

    /// The page length in inches.
    #[must_use]
    pub fn length_inches(&self) -> f64 {
        f64::from(self.page_len.max(1)) / f64::from(self.lpi.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_sizes() {
        let j = PrintJob::default();
        assert_eq!(j.job_text(), "000001/QUSER/QPRTJOB");
        assert_eq!(j.title(), "QSYSPRT.000001.1");
        assert!((j.width_inches() - 13.2).abs() < 1e-9);
        assert!((j.length_inches() - 11.0).abs() < 1e-9);
        assert_eq!(Duplex::parse("*TUMBLE"), Some(Duplex::Tumble));
        assert_eq!(Duplex::from_code(1).name(), "*YES");
    }
}
