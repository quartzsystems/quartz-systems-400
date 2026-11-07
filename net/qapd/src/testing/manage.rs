// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's management (`Manage`, version 7, draft), under the
//! rules of the commands each operation stands for:
//!
//! - `STRSBS` and `ENDSBS` need `*JOBCTL` (`CPF222E`); a description
//!   not there is `CPF9801`, starting an active one `CPF1010`, ending
//!   an inactive one `CPF1054`, ending the controlling subsystem alone
//!   `CPF1034`. `ENDSBS *ALL` leaves the system restricted.
//! - `CHGSYSVAL` as `sec::sysval` rules it: security values need
//!   `*ALLOBJ` and `*SECADM`, auditing values `*AUDIT`, the rest
//!   `*ALLOBJ`; an unknown name is `CPF1842`, a value the value does
//!   not take `TCP1A5F`. A change is audited `SV A name`.
//! - `PWRDWNSYS` needs `*JOBCTL`; once answered the server ends
//!   (`ServerEnding` on every connection), and [`Manage::powered`]
//!   keeps the request.
//! - A special authority missing is audited `AF K user`; a request an
//!   exit program refuses `GR F user` (the point's programs:
//!   [`Manage::exit_refuses`]).
//!
//! The fixtures ([`Manage::shipped`]): `QCTL` (controlling), `QBATCH`,
//! `QINTER`, `QSPL`, `QSYSWRK` and `QUSRWRK` active in `QSYS`, `QSERVER`
//! inactive, `HLINV/HLSBS` inactive; a dozen system values of each type
//! and group; six PTFs in every state; four 1 TB units mirrored in ASP 1
//! and one not configured; the hardware of a small server, the guest
//! agent channel among its `*CMN` resources.

use qap_proto::Status;
use qap_proto::ops::manage::{
    Asp, DiskUnit, DiskUnits, EndSubsystem, HardwareResource, HardwareResources, IplAction,
    IplMode, IplStatus, PowerDown, PreviousEnd, Protection, Ptf, PtfStatus, ResourceStatus,
    ResourceType, SetSystemValue, Subsystem, SubsystemStatus, SystemValue, UnitStatus, ValueKind,
};
use qap_proto::types::{JobId, ObjectName};

use super::spool::No;
use crate::manage::ExitRequest;

/// The groups whose values `sec::sysval` calls security values.
const SECURITY: &str = "SEC";
/// The auditing values (`sec::sysval::AUDIT`).
const AUDIT: &[&str] = &[
    "QAUDCTL",
    "QAUDENDACN",
    "QAUDFRCLVL",
    "QAUDLVL",
    "QAUDLVL2",
    "QCRTOBJAUD",
];

/// A rule an exit program applies: true refuses the request.
pub type ExitRule = Box<dyn Fn(&ExitRequest) -> bool + Send>;

/// One system value and what it takes.
#[derive(Clone, Debug)]
pub struct Value {
    /// The value as the client sees it.
    pub value: SystemValue,
    /// The values it takes; empty for any.
    pub allowed: Vec<&'static str>,
}

/// The test system's management state.
#[derive(Default)]
pub struct Manage {
    /// The subsystem descriptions.
    pub subsystems: Vec<Subsystem>,
    /// The system values, by name.
    pub values: Vec<Value>,
    /// The PTFs.
    pub ptfs: Vec<Ptf>,
    /// The disk units and ASPs.
    pub disks: DiskUnits,
    /// Every hardware resource and the types that list it.
    pub hardware: Vec<(Vec<ResourceType>, HardwareResource)>,
    /// The IPL.
    pub ipl: IplStatus,
    /// The `PowerDown` taken, once one was.
    pub powered: Option<PowerDown>,
    /// The exit program on [`crate::manage::EXIT_POINT`], if one is
    /// registered: true refuses.
    pub exit_refuses: Option<ExitRule>,
    /// Every exit request passed to the point, in `QAPX0100`.
    pub exit_calls: Vec<Vec<u8>>,
    next_job: u32,
}

fn sbsd(lib: &str, name: &str) -> ObjectName {
    ObjectName {
        library: lib.into(),
        name: name.into(),
    }
}

fn val(name: &str, kind: ValueKind, value: &str, group: &str, allowed: &[&'static str]) -> Value {
    Value {
        value: SystemValue {
            name: name.into(),
            kind,
            value: value.into(),
            text: format!("Test system value {name}"),
            category: group.into(),
        },
        allowed: allowed.to_vec(),
    }
}

fn ptf(id: &str, status: PtfStatus, action: IplAction, delayed: bool) -> Ptf {
    Ptf {
        product: "QS00SS1".into(),
        id: id.into(),
        release: "V1R1M0".into(),
        status,
        ipl_action: action,
        delayed,
        text: format!("Test PTF {id}"),
    }
}

fn unit(
    unit: u16,
    asp: u16,
    resource: &str,
    status: UnitStatus,
    protection: Protection,
) -> DiskUnit {
    const TB: u64 = 1_000_204_886_016;
    DiskUnit {
        unit,
        asp,
        resource: resource.into(),
        type_model: "SSD".into(),
        serial: format!("S{resource}"),
        capacity: TB,
        used: if asp == 0 { 0 } else { TB / 4 },
        status,
        protection,
    }
}

fn hw(
    name: &str,
    depth: u8,
    tm: &str,
    text: &str,
    location: &str,
    parent: &str,
) -> HardwareResource {
    HardwareResource {
        name: name.into(),
        depth,
        type_model: tm.into(),
        status: ResourceStatus::Operational,
        text: text.into(),
        location: location.into(),
        serial: String::new(),
        parent: parent.into(),
    }
}

fn subsystems() -> Vec<Subsystem> {
    let active = |name: &str, jobs: u32, n: u32| Subsystem {
        sbsd: sbsd("QSYS", name),
        status: SubsystemStatus::Active,
        jobs,
        monitor: Some(JobId {
            name: name.into(),
            user: "QSYS".into(),
            number: format!("{n:06}"),
        }),
        text: format!("Test subsystem {name}"),
    };
    let inactive = |lib: &str, name: &str| Subsystem {
        sbsd: sbsd(lib, name),
        status: SubsystemStatus::Inactive,
        text: format!("Test subsystem {name}"),
        ..Subsystem::default()
    };
    vec![
        active("QBATCH", 2, 101),
        active("QCTL", 1, 100),
        active("QINTER", 3, 102),
        active("QSPL", 1, 103),
        active("QSYSWRK", 9, 104),
        active("QUSRWRK", 4, 105),
        inactive("HLINV", "HLSBS"),
        inactive("QSYS", "QSERVER"),
    ]
}

fn values() -> Vec<Value> {
    use ValueKind::{Character, Decimal, Integer, List};
    vec![
        val("QAUDLVL", List, "*AUTFAIL *SECURITY", SECURITY, &[]),
        val("QCCSID", Integer, "37", "SYSCTL", &[]),
        val("QCTLSBSD", List, "QCTL QSYS", "SYSCTL", &[]),
        val(
            "QDATFMT",
            Character,
            "MDY",
            "DATTIM",
            &["MDY", "DMY", "YMD", "JUL"],
        ),
        val("QIPLTYPE", Character, "0", "SYSCTL", &["0", "1", "2"]),
        val("QMAXSIGN", Decimal, "3", SECURITY, &[]),
        val("QPWDEXPITV", Decimal, "90", SECURITY, &[]),
        val("QPWDEXPWRN", Decimal, "7", SECURITY, &[]),
        val("QSECURITY", Character, "40", SECURITY, &["30", "40", "50"]),
        val("QSYSLIBL", List, "QSYS QSYS2 QHLPSYS QUSRSYS", "LIBL", &[]),
        val("QTIMSEP", Character, ":", "DATTIM", &[":", ".", ",", " "]),
        val("QUPSDLYTIM", Character, "*CALC", "SYSCTL", &[]),
        val("QDSCJOBITV", Character, "240", "SYSCTL", &[]),
        val(
            "QRMTSIGN",
            Character,
            "*VERIFY",
            SECURITY,
            &["*FRCSIGNON", "*SAMEPRF", "*VERIFY", "*REJECT"],
        ),
        val("QSHRMEMCTL", Character, "0", SECURITY, &["0", "1"]),
    ]
}

fn ptfs() -> Vec<Ptf> {
    use IplAction as A;
    use PtfStatus as P;
    vec![
        ptf("QS00001", P::PermanentlyApplied, A::None, false),
        ptf("QS00002", P::TemporarilyApplied, A::None, false),
        ptf("QS00003", P::NotApplied, A::Apply, true),
        ptf("QS00004", P::Superseded, A::None, false),
        ptf("QS00005", P::TemporarilyApplied, A::Remove, true),
        ptf("QS00006", P::SaveFileOnly, A::None, false),
    ]
}

fn disks() -> DiskUnits {
    DiskUnits {
        units: vec![
            unit(1, 1, "DD001", UnitStatus::Active, Protection::Mirrored),
            unit(1, 1, "DD002", UnitStatus::Active, Protection::Mirrored),
            unit(2, 1, "DD003", UnitStatus::Active, Protection::Mirrored),
            unit(2, 1, "DD004", UnitStatus::Resuming, Protection::Mirrored),
            unit(
                0,
                0,
                "DD005",
                UnitStatus::NonConfigured,
                Protection::Unprotected,
            ),
        ],
        asps: vec![Asp {
            asp: 1,
            capacity: 2 * 1_000_204_886_016,
            used: 1_000_204_886_016 / 2,
            threshold: 90,
        }],
    }
}

fn hardware() -> Vec<(Vec<ResourceType>, HardwareResource)> {
    use ResourceType as T;
    let cmn = || vec![T::All, T::Communication];
    let stg = || vec![T::All, T::Storage];
    let prc = || vec![T::All, T::Processor];
    vec![
        (prc(), hw("PN01", 0, "QP01-001", "Processor", "cpu0", "")),
        (prc(), hw("PN02", 0, "QP01-001", "Processor", "cpu1", "")),
        (
            prc(),
            hw("MS01", 0, "QM01-001", "Main Storage", "memory", ""),
        ),
        (
            vec![T::All, T::Communication, T::Storage],
            hw("CMB01", 0, "QB01-001", "PCI Bridge", "0000:00:00.0", ""),
        ),
        (
            cmn(),
            hw(
                "LIN01",
                1,
                "QN01-001",
                "Network Adapter",
                "0000:00:03.0",
                "CMB01",
            ),
        ),
        (
            cmn(),
            hw(
                "CMN01",
                2,
                "QN01-001",
                "Ethernet Port",
                "0000:00:03.0",
                "LIN01",
            ),
        ),
        (
            cmn(),
            hw("CMN02", 1, "QS01-001", "Serial Port", "ttyS0", "CMB01"),
        ),
        (
            cmn(),
            hw(
                "CMN03",
                1,
                "QV01-001",
                "Virtual Channel",
                "org.qemu.guest_agent.0",
                "CMB01",
            ),
        ),
        (
            stg(),
            hw(
                "DC01",
                1,
                "QD02-001",
                "Storage Controller",
                "0000:00:1f.2",
                "CMB01",
            ),
        ),
        (stg(), hw("DD001", 2, "SSD", "Disk Drive", "sda", "DC01")),
        (stg(), hw("DD002", 2, "SSD", "Disk Drive", "sdb", "DC01")),
        (stg(), hw("DD003", 2, "SSD", "Disk Drive", "sdc", "DC01")),
        (stg(), hw("DD004", 2, "SSD", "Disk Drive", "sdd", "DC01")),
        (stg(), hw("DD005", 2, "SSD", "Disk Drive", "sde", "DC01")),
    ]
}

impl Manage {
    /// The shipped state (see the module).
    #[must_use]
    pub fn shipped() -> Self {
        Self {
            subsystems: subsystems(),
            values: values(),
            ptfs: ptfs(),
            disks: disks(),
            hardware: hardware(),
            ipl: IplStatus {
                ipl_time: 1_792_000_000_000,
                ipl_type: "B".into(),
                mode: IplMode::Normal,
                slot: "B".into(),
                next_slot: None,
                last_src: "C9002A00".into(),
                src_text: "IPL complete".into(),
                previous_end: PreviousEnd::Normal,
                restricted: false,
            },
            powered: None,
            exit_refuses: None,
            exit_calls: Vec::new(),
            next_job: 200,
        }
    }

    /// The descriptions of `library` (`*ALL`, `*LIBL` for `QSYS` and
    /// `QGPL`), active first, then by name.
    #[must_use]
    pub fn list(&self, library: &str) -> Vec<Subsystem> {
        let lib = library.trim().to_ascii_uppercase();
        let mut v: Vec<Subsystem> = self
            .subsystems
            .iter()
            .filter(|s| match lib.as_str() {
                "*ALL" | "" => true,
                "*LIBL" => matches!(s.sbsd.library.as_str(), "QSYS" | "QGPL"),
                l => s.sbsd.library == l,
            })
            .cloned()
            .collect();
        v.sort_by(|a, b| {
            (a.status != SubsystemStatus::Active, &a.sbsd.name)
                .cmp(&(b.status != SubsystemStatus::Active, &b.sbsd.name))
        });
        v
    }

    fn find(&mut self, o: &ObjectName) -> Result<usize, No> {
        let lib = o.library.trim().to_ascii_uppercase();
        let name = o.name.trim().to_ascii_uppercase();
        self.subsystems
            .iter()
            .position(|s| s.sbsd.name == name && (lib == "*LIBL" || s.sbsd.library == lib))
            .ok_or((Status::NotFound, "CPF9801", vec![name, lib]))
    }

    /// `STRSBS`.
    ///
    /// # Errors
    /// As the module says.
    pub fn start(&mut self, o: &ObjectName) -> Result<(), No> {
        let i = self.find(o)?;
        let s = &mut self.subsystems[i];
        if s.status == SubsystemStatus::Active {
            let job = s
                .monitor
                .as_ref()
                .map(|j| j.number.clone())
                .unwrap_or_default();
            return Err((Status::Error, "CPF1010", vec![s.sbsd.name.clone(), job]));
        }
        self.next_job += 1;
        s.status = SubsystemStatus::Active;
        s.monitor = Some(JobId {
            name: s.sbsd.name.clone(),
            user: "QSYS".into(),
            number: format!("{:06}", self.next_job),
        });
        self.ipl.restricted = false;
        Ok(())
    }

    /// `ENDSBS`.
    ///
    /// # Errors
    /// As the module says.
    pub fn end(&mut self, req: &EndSubsystem) -> Result<(), No> {
        let stop = |s: &mut Subsystem| {
            s.status = SubsystemStatus::Inactive;
            s.jobs = 0;
            s.monitor = None;
        };
        let Some(o) = &req.sbsd else {
            self.subsystems.iter_mut().for_each(stop);
            self.ipl.restricted = true;
            return Ok(());
        };
        let i = self.find(o)?;
        let s = &mut self.subsystems[i];
        if s.status != SubsystemStatus::Active {
            return Err((Status::Error, "CPF1054", vec![s.sbsd.name.clone()]));
        }
        if s.sbsd.name == "QCTL" {
            return Err((Status::Error, "CPF1034", vec![s.sbsd.name.clone()]));
        }
        // The test system's jobs end at once, controlled or not.
        stop(s);
        Ok(())
    }

    /// The values asked, in that order (all, by name, when none).
    ///
    /// # Errors
    /// `NotFound` `CPF1842` for a name not there.
    pub fn values(&self, names: &[String]) -> Result<Vec<SystemValue>, No> {
        if names.is_empty() {
            let mut v: Vec<SystemValue> = self.values.iter().map(|v| v.value.clone()).collect();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            return Ok(v);
        }
        names
            .iter()
            .map(|n| {
                let n = n.trim().to_ascii_uppercase();
                self.values
                    .iter()
                    .find(|v| v.value.name == n)
                    .map(|v| v.value.clone())
                    .ok_or((Status::NotFound, "CPF1842", vec![n]))
            })
            .collect()
    }

    /// What `CHGSYSVAL` of `name` needs: the special authorities, all.
    #[must_use]
    pub fn needs(&self, name: &str) -> &'static [&'static str] {
        let security = self
            .values
            .iter()
            .any(|v| v.value.name == name && v.value.category == SECURITY);
        if AUDIT.contains(&name) {
            &["*AUDIT"]
        } else if security {
            &["*ALLOBJ", "*SECADM"]
        } else {
            &["*ALLOBJ"]
        }
    }

    /// `CHGSYSVAL`, authority already checked; the value as it now is.
    ///
    /// # Errors
    /// `CPF1842` for a name not there, `TCP1A5F` for a value not taken.
    pub fn set(&mut self, req: &SetSystemValue) -> Result<SystemValue, No> {
        let name = req.name.trim().to_ascii_uppercase();
        let Some(v) = self.values.iter_mut().find(|v| v.value.name == name) else {
            return Err((Status::NotFound, "CPF1842", vec![name]));
        };
        let new = req.value.trim().to_string();
        let numeric = matches!(v.value.kind, ValueKind::Decimal | ValueKind::Integer);
        let bad = (numeric && new.parse::<i64>().is_err())
            || (!v.allowed.is_empty() && !v.allowed.contains(&new.as_str()))
            || new.is_empty();
        if bad {
            return Err((Status::Error, "TCP1A5F", vec![name, new]));
        }
        v.value.value = new;
        Ok(v.value.clone())
    }

    /// PTFs of `product` (`*ALL`), by product then identifier; an
    /// unknown product lists none.
    #[must_use]
    pub fn ptfs(&self, product: &str) -> Vec<Ptf> {
        let p = product.trim().to_ascii_uppercase();
        let mut v: Vec<Ptf> = self
            .ptfs
            .iter()
            .filter(|f| p == "*ALL" || p.is_empty() || f.product == p)
            .cloned()
            .collect();
        v.sort_by(|a, b| (&a.product, &a.id).cmp(&(&b.product, &b.id)));
        v
    }

    /// The resources a type lists, in tree order.
    #[must_use]
    pub fn hardware(&self, kind: ResourceType) -> HardwareResources {
        HardwareResources {
            resources: self
                .hardware
                .iter()
                .filter(|(t, _)| t.contains(&kind))
                .map(|(_, r)| r.clone())
                .collect(),
        }
    }

    /// Whether the exit program refuses `r`; the call is kept.
    pub fn exit(&mut self, r: &ExitRequest) -> bool {
        self.exit_calls.push(r.format());
        self.exit_refuses.as_ref().is_some_and(|f| f(r))
    }
}
