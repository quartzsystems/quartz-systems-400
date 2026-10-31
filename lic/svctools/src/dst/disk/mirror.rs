// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with mirrored protection`: start and stop.

use crate::dst::common::{Confirm, Select, Submenu};
use crate::dst::disk::{self, STATUS_HEADINGS};
use crate::env::{DiskUnit, DstEnv, NonConfigured};
use crate::panel::Flow;

/// The `Work with Mirrored Protection` menu.
#[must_use]
pub fn work_with_mirrored_protection() -> Submenu {
    Submenu::new(
        "dst-mirror",
        "Work with Mirrored Protection",
        vec![
            (
                1,
                "Display disk configuration",
                Some(Box::new(|env| {
                    Flow::Push(Box::new(disk::display(
                        "dst-disk-status",
                        "Display Disk Configuration Status",
                        STATUS_HEADINGS,
                        env,
                        disk::status_row,
                    )))
                })),
            ),
            (
                2,
                "Start mirrored protection",
                Some(Box::new(|env| Flow::Push(Box::new(start(env))))),
            ),
            (
                3,
                "Stop mirrored protection",
                Some(Box::new(|env| Flow::Push(Box::new(stop(env))))),
            ),
            (4, "Enable remote load source mirroring", None),
        ],
    )
}

/// One pair start mirrored protection forms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pairing {
    /// The unit that keeps its data and becomes the first half.
    pub unit: DiskUnit,
    /// The device that becomes the second half.
    pub partner: NonConfigured,
    /// The configured unit the partner is now: its data moves to the
    /// other units before the pair forms.
    pub from_unit: Option<u16>,
    /// The protection level: `Controller` when the halves sit behind
    /// different controllers, else `Device`.
    pub level: &'static str,
}

/// Why start mirrored protection cannot pair the ASP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Every unit is already half of a pair.
    NothingToPair,
    /// This many units are left without a partner: not an even number.
    OddCount(usize),
    /// No partner of sufficient size for this unit.
    NoPartner(u16),
    /// This unit would have to leave the configuration but another
    /// unit after it stays.
    CannotLeave(u16),
    /// The data on the units to pair does not fit on their first halves.
    NoRoom {
        /// MiB in use on the units to pair.
        used_mib: u64,
        /// MiB of the units that keep their data.
        room_mib: u64,
    },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NothingToPair => write!(f, "Every unit in ASP 1 already has mirrored protection"),
            Self::OddCount(n) => write!(
                f,
                "{n} units have no partner; mirroring needs an even number of units"
            ),
            Self::NoPartner(u) => {
                write!(f, "No unit or disk of sufficient size can pair with unit {}", u + 1)
            }
            Self::CannotLeave(u) => write!(
                f,
                "Unit {} cannot become a partner: a unit after it stays unpaired",
                u + 1
            ),
            Self::NoRoom { used_mib, room_mib } => write!(
                f,
                "{used_mib} MiB in use does not fit in half the units, {room_mib} MiB"
            ),
        }
    }
}

/// The protection a pair gets from where its halves are attached.
#[must_use]
pub fn level(a: &str, b: &str) -> &'static str {
    let known = |c: &str| !c.is_empty() && !c.starts_with('-');
    if known(a) && known(b) && a != b {
        "Controller"
    } else {
        "Device"
    }
}

/// MiB in use on a unit, rounded up.
fn used_mib(u: &DiskUnit) -> u64 {
    (u.size_mib * u64::from(u.percent_used)).div_ceil(100)
}

/// The first candidate at least `size` MiB, one behind another
/// controller preferred.
fn best<'a>(size: u64, controller: &str, cands: impl Iterator<Item = (u64, &'a str)>) -> Option<usize> {
    let fits: Vec<(usize, &str)> = cands
        .enumerate()
        .filter(|(_, (s, _))| *s >= size)
        .map(|(i, (_, c))| (i, c))
        .collect();
    fits.iter()
        .find(|(_, c)| level(controller, c) == "Controller")
        .or_else(|| fits.first())
        .map(|(i, _)| *i)
}

/// A configured unit as the device that becomes a second half.
fn as_disk(u: &DiskUnit) -> NonConfigured {
    NonConfigured {
        device: u.device.clone(),
        resource: u.resource.clone(),
        serial: u.serial.clone(),
        type_model: u.type_model.clone(),
        size_mib: u.size_mib,
        has_data: u.percent_used > 0,
        controller: u.controller.clone(),
    }
}

/// Pair every unprotected unit, unit 1 first. Non-configured disks at
/// least as large are taken first. The units left pair with each other:
/// the lower half of them keep their data, the upper half become their
/// partners, giving up their data to the other units first, the highest
/// paired with the lowest. That needs an even count, partners at the
/// end of the configuration, and the data in use fitting on the units
/// that keep it. A partner behind another controller is preferred.
pub fn propose_pairs(units: &[DiskUnit], spare: &[NonConfigured]) -> Result<Vec<Pairing>, Refusal> {
    let mut alone: Vec<&DiskUnit> = units.iter().filter(|u| !u.mirrored).collect();
    alone.sort_by_key(|u| u.unit);
    if alone.is_empty() {
        return Err(Refusal::NothingToPair);
    }
    let mut free: Vec<NonConfigured> = spare.to_vec();
    let mut pairs = Vec::new();
    let mut rest: Vec<&DiskUnit> = Vec::new();
    for u in alone {
        let pick = best(
            u.size_mib,
            &u.controller,
            free.iter().map(|d| (d.size_mib, d.controller.as_str())),
        );
        if let Some(i) = pick {
            let d = free.remove(i);
            pairs.push(Pairing {
                level: level(&u.controller, &d.controller),
                unit: u.clone(),
                partner: d,
                from_unit: None,
            });
        } else {
            rest.push(u);
        }
    }
    if rest.len() % 2 == 1 {
        return Err(Refusal::OddCount(rest.len()));
    }
    let (firsts, seconds) = rest.split_at(rest.len() / 2);
    // Partners leave from the end: after the lowest of them only other
    // partners and second halves of existing pairs may follow.
    if let Some(low) = seconds.first().map(|u| u.unit) {
        let blocked = units.iter().any(|x| {
            x.unit > low
                && !seconds.iter().any(|s| s.unit == x.unit)
                && !(x.mirrored && x.partner.is_some_and(|p| p < x.unit))
        });
        if blocked {
            return Err(Refusal::CannotLeave(low));
        }
    }
    let used: u64 = rest.iter().map(|u| used_mib(u)).sum();
    let room: u64 = firsts.iter().map(|u| u.size_mib).sum();
    if used > room {
        return Err(Refusal::NoRoom {
            used_mib: used,
            room_mib: room,
        });
    }
    let mut seconds: Vec<&DiskUnit> = seconds.iter().rev().copied().collect();
    for u in firsts {
        let i = best(
            u.size_mib,
            &u.controller,
            seconds.iter().map(|s| (s.size_mib, s.controller.as_str())),
        )
        .ok_or(Refusal::NoPartner(u.unit))?;
        let s = seconds.remove(i);
        pairs.push(Pairing {
            level: level(&u.controller, &s.controller),
            unit: (*u).clone(),
            partner: as_disk(s),
            from_unit: Some(s.unit),
        });
    }
    pairs.sort_by_key(|p| p.unit.unit);
    Ok(pairs)
}

/// The order the pairs form in: partners that are configured units
/// first, highest unit first, since only the last unit can leave.
#[must_use]
pub fn forming_order(pairs: &[Pairing]) -> Vec<(u16, String)> {
    let mut order: Vec<&Pairing> = pairs.iter().collect();
    order.sort_by_key(|p| std::cmp::Reverse(p.from_unit));
    order
        .iter()
        .map(|p| (p.unit.unit, p.partner.device.clone()))
        .collect()
}

/// `ASP Unit  Serial Number     Type Model    Resource  Protection`.
pub const PAIR_HEADINGS: &str = "ASP Unit  Serial Number     Type Model    Resource  Protection";

/// The two rows of a pair, both under the first half's unit number.
#[must_use]
pub fn pair_rows(p: &Pairing) -> [String; 2] {
    let row = |serial: &str, type_model: &str, resource: &str, note: &str| {
        format!(
            "{:>3} {:>4}  {:<17} {:<13} {:<9} {}{note}",
            p.unit.asp,
            p.unit.unit + 1,
            disk::clip(serial, 17),
            disk::clip(type_model, 13),
            resource,
            p.level
        )
    };
    let d = &p.partner;
    [
        row(&p.unit.serial, &p.unit.type_model, &p.unit.resource, ""),
        row(
            &d.serial,
            &d.type_model,
            &d.resource,
            if p.from_unit.is_some() { "  data moves" } else { "" },
        ),
    ]
}

fn start(_env: &mut dyn DstEnv) -> Select {
    let rows = vec!["  1  ASP 1 (system ASP)".to_string()];
    Select::new(
        "dst-mirror-start-asp",
        "Select ASP to Start Mirrored Protection",
        "Type 1 beside the ASP, then press Enter.",
        "Opt  ASP",
        rows,
        Box::new(|_chosen: &[usize], env: &mut dyn DstEnv| {
            let units = env.disk_units();
            let spare = env.nonconfigured();
            let pairs = match propose_pairs(&units, &spare) {
                Ok(p) => p,
                Err(why) => return Flow::Error(why.to_string()),
            };
            let mut lines = vec![
                "Press F10 to form these pairs. The data on a second unit marked".to_string(),
                "\"data moves\" goes to the other units first; each second unit is".into(),
                "then copied from its first at the next IPL.".into(),
                String::new(),
                format!("  {PAIR_HEADINGS}"),
            ];
            for p in &pairs {
                for row in pair_rows(p) {
                    lines.push(format!("  {row}"));
                }
            }
            let plan = forming_order(&pairs);
            Flow::Push(Box::new(
                Confirm::new(
                    "dst-mirror-start-confirm",
                    "Confirm Start Mirrored Protection",
                    lines,
                    Box::new(move |env: &mut dyn DstEnv| {
                        let mut formed = 0;
                        for (unit, device) in &plan {
                            env.start_mirror(*unit, device).map_err(|e| {
                                if formed == 0 {
                                    e
                                } else {
                                    format!("{formed} pair(s) formed, then: {e}")
                                }
                            })?;
                            formed += 1;
                        }
                        Ok(format!(
                            "{formed} pair(s) formed; mirrored protection synchronization runs at the next IPL"
                        ))
                    }),
                )
                .unwinding(2),
            ))
        }),
    )
}

fn stop(env: &mut dyn DstEnv) -> Select {
    let units: Vec<DiskUnit> = env
        .disk_units()
        .into_iter()
        .filter(|u| u.mirrored)
        .collect();
    let rows: Vec<String> = units.iter().map(disk::status_row).collect();
    Select::new(
        "dst-mirror-stop",
        "Stop Mirrored Protection",
        "Type 1 beside the half to keep, then press Enter. Its partner leaves the configuration.",
        &format!("Opt  {STATUS_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            let keep: Vec<u16> = chosen.iter().map(|&i| units[i].unit).collect();
            let mut lines = vec![
                "Mirrored protection stops for these units; the other half of each".to_string(),
                "pair is removed from the configuration:".into(),
                String::new(),
            ];
            lines.extend(
                chosen
                    .iter()
                    .map(|&i| format!("  {}", disk::status_row(&units[i]))),
            );
            Flow::Push(Box::new(
                Confirm::new(
                    "dst-mirror-stop-confirm",
                    "Confirm Stop Mirrored Protection",
                    lines,
                    Box::new(move |env: &mut dyn DstEnv| {
                        for k in &keep {
                            env.stop_mirror(*k)?;
                        }
                        Ok(format!(
                            "Mirrored protection stopped for {} unit(s)",
                            keep.len()
                        ))
                    }),
                )
                .unwinding(2),
            ))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(n: u16, percent: u8, controller: &str) -> DiskUnit {
        DiskUnit {
            asp: 1,
            unit: n,
            serial: format!("S{n}"),
            type_model: "SAMSUNG SSD".into(),
            resource: format!("DD00{}", n + 1),
            size_mib: 1000,
            percent_used: percent,
            status: "Unprotected".into(),
            mirrored: false,
            partner: None,
            attached: true,
            resume_percent: None,
            device: format!("/dev/sd{}", (b'a' + u8::try_from(n).unwrap()) as char),
            controller: controller.into(),
        }
    }

    fn paired(mut u: DiskUnit, partner: u16) -> DiskUnit {
        u.mirrored = true;
        u.partner = Some(partner);
        u.status = "Active".into();
        u
    }

    fn spare(name: &str, size: u64) -> NonConfigured {
        NonConfigured {
            device: format!("/dev/{name}"),
            resource: "DD009".into(),
            serial: name.into(),
            type_model: "SAMSUNG SSD".into(),
            size_mib: size,
            has_data: false,
            controller: "DC02".into(),
        }
    }

    fn summary(pairs: &[Pairing]) -> Vec<(u16, String, Option<u16>, &'static str)> {
        pairs
            .iter()
            .map(|p| (p.unit.unit, p.partner.device.clone(), p.from_unit, p.level))
            .collect()
    }

    #[test]
    fn four_units_holding_data_pair_with_each_other() {
        let units: Vec<DiskUnit> = (0..4).map(|n| unit(n, 20, "DC01")).collect();
        let pairs = propose_pairs(&units, &[]).unwrap();
        assert_eq!(
            summary(&pairs),
            vec![
                (0, "/dev/sdd".into(), Some(3), "Device"),
                (1, "/dev/sdc".into(), Some(2), "Device"),
            ]
        );
        assert!(pairs.iter().all(|p| p.partner.has_data));
        // The highest partner leaves first.
        assert_eq!(
            forming_order(&pairs),
            vec![(0, "/dev/sdd".to_string()), (1, "/dev/sdc".to_string())]
        );
        let rows = pair_rows(&pairs[0]);
        assert!(rows[0].starts_with("  1    1  S0"), "{}", rows[0]);
        assert!(rows[1].starts_with("  1    1  S3"), "{}", rows[1]);
        assert!(rows[1].ends_with("Device  data moves"), "{}", rows[1]);
    }

    #[test]
    fn a_partial_pairing_is_completed() {
        // Units 1 and 4 already pair; 2 and 3 hold data.
        let units = vec![
            paired(unit(0, 20, "DC01"), 3),
            unit(1, 30, "DC01"),
            unit(2, 30, "DC01"),
            paired(unit(3, 20, "DC01"), 0),
        ];
        let pairs = propose_pairs(&units, &[]).unwrap();
        assert_eq!(summary(&pairs), vec![(1, "/dev/sdc".into(), Some(2), "Device")]);
    }

    #[test]
    fn another_controller_is_preferred() {
        let units = vec![
            unit(0, 10, "DC01"),
            unit(1, 10, "DC02"),
            unit(2, 10, "DC02"),
            unit(3, 10, "DC01"),
        ];
        let pairs = propose_pairs(&units, &[]).unwrap();
        assert_eq!(
            summary(&pairs),
            vec![
                (0, "/dev/sdc".into(), Some(2), "Controller"),
                (1, "/dev/sdd".into(), Some(3), "Controller"),
            ]
        );
        assert_eq!(forming_order(&pairs)[0], (1, "/dev/sdd".to_string()));
    }

    #[test]
    fn an_odd_count_is_refused() {
        let units: Vec<DiskUnit> = (0..3).map(|n| unit(n, 10, "DC01")).collect();
        assert_eq!(propose_pairs(&units, &[]), Err(Refusal::OddCount(3)));
        let units = vec![
            paired(unit(0, 20, "DC01"), 3),
            unit(1, 30, "DC01"),
            unit(2, 30, "DC01"),
            paired(unit(3, 20, "DC01"), 0),
            unit(4, 0, "DC01"),
        ];
        assert_eq!(propose_pairs(&units, &[]), Err(Refusal::OddCount(3)));
    }

    #[test]
    fn data_that_does_not_fit_in_half_is_refused() {
        // 4 x 600 MiB in use, 2000 MiB left once two units leave.
        let units: Vec<DiskUnit> = (0..4).map(|n| unit(n, 60, "DC01")).collect();
        assert_eq!(
            propose_pairs(&units, &[]),
            Err(Refusal::NoRoom {
                used_mib: 2400,
                room_mib: 2000
            })
        );
        let half: Vec<DiskUnit> = (0..4).map(|n| unit(n, 50, "DC01")).collect();
        assert!(propose_pairs(&half, &[]).is_ok());
    }

    #[test]
    fn nonconfigured_disks_are_taken_first() {
        let units: Vec<DiskUnit> = (0..3).map(|n| unit(n, 10, "DC01")).collect();
        let pairs = propose_pairs(&units, &[spare("sdx", 1000)]).unwrap();
        assert_eq!(
            summary(&pairs),
            vec![
                (0, "/dev/sdx".into(), None, "Controller"),
                (1, "/dev/sdc".into(), Some(2), "Device"),
            ]
        );
        // Spares for every unit: nothing leaves the configuration.
        let pairs = propose_pairs(&units[..1], &[spare("sdx", 500), spare("sdy", 1000)]).unwrap();
        assert_eq!(summary(&pairs), vec![(0, "/dev/sdy".into(), None, "Controller")]);
    }

    #[test]
    fn a_partner_must_be_able_to_leave() {
        // Unit 3 takes the small spare and stays, so unit 2 cannot leave.
        let mut units: Vec<DiskUnit> = (0..3).map(|n| unit(n, 10, "DC01")).collect();
        units[2].size_mib = 400;
        assert_eq!(
            propose_pairs(&units, &[spare("sdx", 400)]),
            Err(Refusal::CannotLeave(1))
        );
    }

    #[test]
    fn a_smaller_partner_is_refused() {
        let mut units: Vec<DiskUnit> = (0..2).map(|n| unit(n, 10, "DC01")).collect();
        units[1].size_mib = 500;
        assert_eq!(propose_pairs(&units, &[]), Err(Refusal::NoPartner(0)));
    }

    #[test]
    fn nothing_left_to_pair() {
        let units = vec![paired(unit(0, 10, "DC01"), 1), paired(unit(1, 10, "DC01"), 0)];
        assert_eq!(propose_pairs(&units, &[]), Err(Refusal::NothingToPair));
    }
}
