// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What TCP/IP needs to know of line descriptions. [`Described`] is
//! communications configuration's answer (`*LIND`, `VRYCFG`): a line
//! description's status and interface, and for a name no description
//! has, [`KernelNames`]: a line named like a kernel link (`LIND(ETH0)`
//! for `eth0`) exists and is varied on.

/// A line description as TCP/IP sees it.
pub trait LineState {
    /// Whether line description `lind` exists.
    fn exists(&self, lind: &str) -> bool;
    /// The kernel link the line runs on, when it has one.
    fn interface(&self, lind: &str) -> Option<String>;
    /// Whether the line is varied on.
    fn varied_on(&self, lind: &str) -> bool;
    /// Vary the line on, as `STRTCPIFC` does for a line that is off.
    fn vary_on(&mut self, lind: &str) -> Result<(), String>;
    /// The line type (`*ELAN` for Ethernet).
    fn line_type(&self, lind: &str) -> Option<String> {
        self.exists(lind).then(|| "*ELAN".to_string())
    }
}

/// No line descriptions at all.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoLines;

impl LineState for NoLines {
    fn exists(&self, _: &str) -> bool {
        false
    }

    fn interface(&self, _: &str) -> Option<String> {
        None
    }

    fn varied_on(&self, _: &str) -> bool {
        false
    }

    fn vary_on(&mut self, lind: &str) -> Result<(), String> {
        Err(format!("line {lind} does not exist"))
    }
}

/// Lines named after kernel links: `LIND(ETH0)` is `eth0`. Every line
/// is varied on. A stand-in for the communications objects.
#[derive(Clone, Debug, Default)]
pub struct KernelNames {
    /// The kernel's link names.
    pub links: Vec<String>,
}

impl KernelNames {
    /// The lines over these links.
    pub fn new(links: impl IntoIterator<Item = String>) -> Self {
        Self {
            links: links.into_iter().filter(|l| l != "lo").collect(),
        }
    }
}

impl LineState for KernelNames {
    fn exists(&self, lind: &str) -> bool {
        self.interface(lind).is_some()
    }

    fn interface(&self, lind: &str) -> Option<String> {
        self.links.iter().find(|l| l.eq_ignore_ascii_case(lind)).cloned()
    }

    fn varied_on(&self, lind: &str) -> bool {
        self.exists(lind)
    }

    fn vary_on(&mut self, lind: &str) -> Result<(), String> {
        if self.exists(lind) {
            Ok(())
        } else {
            Err(format!("line {lind} does not exist"))
        }
    }
}

/// A line description as communications configuration holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineInfo {
    /// Varied on (any status but varied off).
    pub on: bool,
    /// The kernel interface it is varied on over; blank until then.
    pub ifname: String,
}

/// Looks a line description up by name.
pub type FindLine<'a> = Box<dyn Fn(&str) -> Option<LineInfo> + 'a>;
/// Varies a line description on.
pub type VaryLine<'a> = Box<dyn FnMut(&str) -> Result<(), String> + 'a>;

/// The line descriptions, and for a name none has, a kernel link of
/// that name ([`KernelNames`]). A line follows its description: it is
/// on only while varied on, and its interface is the one it was varied
/// on over.
pub struct Described<'a> {
    find: FindLine<'a>,
    vary: VaryLine<'a>,
    kernel: KernelNames,
}

impl<'a> Described<'a> {
    /// Lines from `find`, varied on through `vary`, with `kernel` for
    /// names no description has.
    #[must_use]
    pub fn new(find: FindLine<'a>, vary: VaryLine<'a>, kernel: KernelNames) -> Self {
        Self { find, vary, kernel }
    }
}

impl LineState for Described<'_> {
    fn exists(&self, lind: &str) -> bool {
        (self.find)(lind).is_some() || self.kernel.exists(lind)
    }

    fn interface(&self, lind: &str) -> Option<String> {
        match (self.find)(lind) {
            Some(l) => (l.on && !l.ifname.is_empty()).then_some(l.ifname),
            None => self.kernel.interface(lind),
        }
    }

    fn varied_on(&self, lind: &str) -> bool {
        match (self.find)(lind) {
            Some(l) => l.on,
            None => self.kernel.varied_on(lind),
        }
    }

    fn vary_on(&mut self, lind: &str) -> Result<(), String> {
        if (self.find)(lind).is_some() {
            (self.vary)(lind)
        } else {
            self.kernel.vary_on(lind)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use super::*;

    fn kernel() -> KernelNames {
        KernelNames::new(["lo".to_string(), "eth0".to_string(), "eth1".to_string()])
    }

    #[test]
    fn a_description_decides_its_line() {
        let lines = RefCell::new(BTreeMap::from([("ETHLINE".to_string(), LineInfo::default())]));
        let mut d = Described::new(
            Box::new(|n| lines.borrow().get(n).cloned()),
            Box::new(|n| {
                let mut m = lines.borrow_mut();
                let l = m.get_mut(n).ok_or("no line")?;
                l.on = true;
                l.ifname = "eth0".into();
                Ok(())
            }),
            kernel(),
        );
        assert!(d.exists("ETHLINE"));
        assert!(!d.varied_on("ETHLINE"));
        assert_eq!(d.interface("ETHLINE"), None);
        d.vary_on("ETHLINE").unwrap();
        assert!(d.varied_on("ETHLINE"));
        assert_eq!(d.interface("ETHLINE").as_deref(), Some("eth0"));
        // Varied off: the line, and so its interfaces, follow.
        lines.borrow_mut().get_mut("ETHLINE").unwrap().on = false;
        assert!(!d.varied_on("ETHLINE"));
        assert_eq!(d.interface("ETHLINE"), None);
    }

    #[test]
    fn kernel_names_without_a_description() {
        let mut d = Described::new(Box::new(|_| None), Box::new(|_| Err("not called".into())), kernel());
        assert!(d.exists("ETH1"));
        assert!(d.varied_on("ETH1"));
        assert_eq!(d.interface("ETH1").as_deref(), Some("eth1"));
        assert!(d.vary_on("ETH1").is_ok());
        assert!(!d.exists("LO"));
        assert!(!d.exists("NOSUCH"));
        assert!(d.vary_on("NOSUCH").is_err());
    }

    #[test]
    fn a_description_hides_the_kernel_name() {
        // LIND(ETH0) described but varied off is off, though eth0 is up.
        let d = Described::new(
            Box::new(|n| (n == "ETH0").then(LineInfo::default)),
            Box::new(|_| Ok(())),
            kernel(),
        );
        assert!(d.exists("ETH0"));
        assert!(!d.varied_on("ETH0"));
        assert_eq!(d.interface("ETH0"), None);
    }
}
