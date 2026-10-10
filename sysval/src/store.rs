// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The live set of system values: catalog defaults, overrides stored in
//! the LIC record, system-set values, and the clock-derived ones.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use licpart::{Device, LicPart, Record};

use crate::catalog::{Catalog, Entry};
use crate::clock::LocalTime;
use crate::error::SysValError;
use crate::tz;
use crate::value::Value;

/// TLV tag holding the overrides. MIGRATE(phase2).
pub const TAG_SYSVALS: u16 = 0x0100;

/// What a successful `set` means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// The change is in effect now.
    Immediate,
    /// The change takes effect at the next IPL.
    NextIpl,
}

/// Facts about the machine that feed system-set values.
#[derive(Clone, Debug, Default)]
pub struct HostInfo {
    /// DMI product UUID, if readable.
    pub product_uuid: Option<String>,
}

/// The system values.
pub struct SysVals {
    catalog: Catalog,
    /// User overrides: values that differ from the default, persisted.
    overrides: BTreeMap<String, Value>,
    /// System-set values for this IPL, not persisted.
    system: BTreeMap<String, Value>,
}

impl SysVals {
    /// Catalog defaults plus whatever the record stores.
    pub fn from_record(record: &Record, host: &HostInfo) -> Result<Self, SysValError> {
        let catalog = Catalog::builtin()?;
        let mut vals = Self {
            catalog,
            overrides: BTreeMap::new(),
            system: BTreeMap::new(),
        };
        if let Some(data) = record.tlv(TAG_SYSVALS) {
            vals.overrides = decode(data, &vals.catalog)?;
        }
        if let Some(uuid) = &host.product_uuid {
            vals.system
                .insert("QSRLNBR".into(), Value::Char(serial_from_uuid(uuid)));
        }
        Ok(vals)
    }

    /// Read the newest record from the partition and load from it.
    pub fn load<D: Device>(
        part: &mut LicPart<D>,
        host: &HostInfo,
    ) -> Result<(Self, Record), SysValError> {
        let record = part.read().map_err(|e| SysValError::Lic(e.to_string()))?;
        let vals = Self::from_record(&record, host)?;
        Ok((vals, record))
    }

    /// Put the overrides into the record. Write the record afterwards.
    pub fn store(&self, record: &mut Record) -> Result<(), SysValError> {
        record
            .set_tlv(TAG_SYSVALS, encode(&self.overrides))
            .map_err(|e| SysValError::Lic(e.to_string()))
    }

    /// Store into the record and write it to the partition.
    pub fn save<D: Device>(
        &self,
        part: &mut LicPart<D>,
        record: &mut Record,
    ) -> Result<(), SysValError> {
        self.store(record)?;
        part.write(record)
            .map(|_| ())
            .map_err(|e| SysValError::Lic(e.to_string()))
    }

    /// The catalog.
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// Current value. Clock-derived values are computed from the system clock.
    pub fn get(&self, name: &str) -> Result<Value, SysValError> {
        self.get_at(name, now_unix())
    }

    /// Current value with the clock pinned to `unix` (UTC).
    pub fn get_at(&self, name: &str, unix: u64) -> Result<Value, SysValError> {
        let entry = self.entry(name)?;
        if let Some(v) = self.derived(name, unix) {
            return Ok(v);
        }
        if let Some(v) = self.system.get(name) {
            return Ok(v.clone());
        }
        if let Some(v) = self.overrides.get(name) {
            return Ok(v.clone());
        }
        Ok(entry.default_value())
    }

    /// Change a value as a user: validated, read-only refused. Persist
    /// with `store`/`save` afterwards.
    pub fn set(&mut self, name: &str, value: Value) -> Result<Effect, SysValError> {
        let entry = self.entry(name)?;
        if entry.readonly {
            return Err(SysValError::ReadOnly(name.to_string()));
        }
        entry.validate(&value)?;
        if name == "QTIMZON"
            && let Value::Char(z) = &value
            && tz::find(z).is_none()
        {
            return Err(SysValError::NotAllowed {
                name: name.to_string(),
                got: z.clone(),
                allowed: tz::ZONES
                    .iter()
                    .map(|z| z.name)
                    .collect::<Vec<_>>()
                    .join(", "),
            });
        }
        let effect = if entry.restart_required {
            Effect::NextIpl
        } else {
            Effect::Immediate
        };
        if value == entry.default_value() {
            self.overrides.remove(name);
        } else {
            self.overrides.insert(name.to_string(), value);
        }
        Ok(effect)
    }

    /// Parse text in the entry's type and `set` it.
    pub fn set_text(&mut self, name: &str, text: &str) -> Result<Effect, SysValError> {
        let value = self.entry(name)?.parse_value(text)?;
        self.set(name, value)
    }

    /// Change a value as the system: validated, read-only allowed, not
    /// persisted (it describes this IPL).
    pub fn set_system(&mut self, name: &str, value: Value) -> Result<(), SysValError> {
        self.entry(name)?.validate(&value)?;
        self.system.insert(name.to_string(), value);
        Ok(())
    }

    /// The zone QTIMZON names.
    pub fn zone(&self) -> &'static tz::Zone {
        let name = self
            .overrides
            .get("QTIMZON")
            .map_or_else(|| "QN0000UTC".to_string(), Value::text);
        tz::find(&name).unwrap_or(&tz::ZONES[0])
    }

    /// Local time now according to QTIMZON.
    pub fn local_time(&self, unix: u64) -> LocalTime {
        LocalTime::from_unix(unix, self.zone().offset_at(unix))
    }

    /// Names with a stored override.
    pub fn overridden(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.overrides.iter()
    }

    fn entry(&self, name: &str) -> Result<&Entry, SysValError> {
        self.catalog
            .get(name)
            .ok_or_else(|| SysValError::Unknown(name.to_string()))
    }

    fn derived(&self, name: &str, unix: u64) -> Option<Value> {
        let fmt = self
            .overrides
            .get("QDATFMT")
            .map_or("MDY".to_string(), Value::text);
        let t = self.local_time(unix);
        let v = match name {
            "QDATE" => t.qdate(&fmt),
            "QDATETIME" => t.qdatetime(),
            "QDAY" => t.qday(&fmt),
            "QDAYOFWEEK" => t.qdayofweek().to_string(),
            "QHOUR" => format!("{:02}", t.hour),
            "QMINUTE" => format!("{:02}", t.minute),
            "QSECOND" => format!("{:02}", t.second),
            "QMONTH" => format!("{:02}", t.month),
            "QYEAR" => format!("{:02}", t.year.rem_euclid(100)),
            "QCENTURY" => t.qcentury().to_string(),
            "QTIME" => t.qtime(),
            "QUTCOFFSET" => self.zone().utc_offset_text(unix),
            _ => return None,
        };
        Some(Value::Char(v))
    }
}

/// Seconds since the Unix epoch.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Seven characters from a hash of the machine's product UUID.
pub fn serial_from_uuid(uuid: &str) -> String {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKLMNPQRSTUVWXYZ";
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in uuid.trim().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    let mut out = String::with_capacity(7);
    for _ in 0..7 {
        let idx = usize::try_from(h % ALPHABET.len() as u64).unwrap_or(0);
        out.push(ALPHABET[idx] as char);
        h /= ALPHABET.len() as u64;
    }
    out
}

// Storage: count u16, then per entry name_len u8, name, value_len u16, value text.
// Lists are stored space separated and re-split on load.

fn encode(overrides: &BTreeMap<String, Value>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(
        &u16::try_from(overrides.len())
            .unwrap_or(u16::MAX)
            .to_le_bytes(),
    );
    for (name, value) in overrides {
        let text = value.text();
        out.push(u8::try_from(name.len()).unwrap_or(u8::MAX));
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&u16::try_from(text.len()).unwrap_or(u16::MAX).to_le_bytes());
        out.extend_from_slice(text.as_bytes());
    }
    out
}

fn decode(data: &[u8], catalog: &Catalog) -> Result<BTreeMap<String, Value>, SysValError> {
    let bad = SysValError::Storage("truncated");
    let mut map = BTreeMap::new();
    let count = u16::from_le_bytes(data.get(..2).ok_or(bad.clone())?.try_into().expect("2"));
    let mut pos = 2;
    for _ in 0..count {
        let nlen = usize::from(*data.get(pos).ok_or(bad.clone())?);
        pos += 1;
        let name = std::str::from_utf8(data.get(pos..pos + nlen).ok_or(bad.clone())?)
            .map_err(|_| SysValError::Storage("name not utf-8"))?
            .to_string();
        pos += nlen;
        let vlen = usize::from(u16::from_le_bytes(
            data.get(pos..pos + 2)
                .ok_or(bad.clone())?
                .try_into()
                .expect("2"),
        ));
        pos += 2;
        let text = std::str::from_utf8(data.get(pos..pos + vlen).ok_or(bad.clone())?)
            .map_err(|_| SysValError::Storage("value not utf-8"))?;
        pos += vlen;
        // Unknown names (from a newer catalog) are dropped silently.
        if let Some(entry) = catalog.get(&name)
            && let Ok(v) = entry.parse_value(text)
            && entry.validate(&v).is_ok()
        {
            map.insert(name, v);
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::civil_to_unix;

    fn fresh() -> SysVals {
        SysVals::from_record(&Record::new(0), &HostInfo::default()).unwrap()
    }

    #[test]
    fn defaults_and_overrides() {
        let mut v = fresh();
        assert_eq!(v.get("QMODEL").unwrap().text(), "QS40");
        assert_eq!(v.get("QSECURITY").unwrap().text(), "40");
        assert_eq!(v.set_text("QSECURITY", "50").unwrap(), Effect::NextIpl);
        assert_eq!(v.get("QSECURITY").unwrap().text(), "50");
        assert_eq!(v.set_text("QSYSNAME", "DEV01").unwrap(), Effect::Immediate);
        assert_eq!(
            v.set_text("QMODEL", "X"),
            Err(SysValError::ReadOnly("QMODEL".into()))
        );
        assert!(matches!(
            v.set_text("QNOPE", "1"),
            Err(SysValError::Unknown(_))
        ));
        assert!(matches!(
            v.set_text("QSECURITY", "60"),
            Err(SysValError::NotAllowed { .. })
        ));
        assert!(matches!(
            v.set_text("QTIMZON", "QX9999XXX"),
            Err(SysValError::NotAllowed { .. })
        ));
    }

    #[test]
    fn overrides_round_trip_through_the_record() {
        let mut v = fresh();
        v.set_text("QSECURITY", "50").unwrap();
        v.set_text("QSYSLIBL", "QSYS QSYS2 QUSRSYS").unwrap();
        v.set_text("QTIMZON", "QN0500EST2").unwrap();
        let mut rec = Record::new(0);
        v.store(&mut rec).unwrap();
        let back = SysVals::from_record(&rec, &HostInfo::default()).unwrap();
        assert_eq!(back.get("QSECURITY").unwrap().text(), "50");
        assert_eq!(
            back.get("QSYSLIBL").unwrap(),
            Value::List(vec!["QSYS".into(), "QSYS2".into(), "QUSRSYS".into()])
        );
        assert_eq!(back.overridden().count(), 3);
        // Setting back to the default drops the override.
        let mut back = back;
        back.set_text("QSECURITY", "40").unwrap();
        assert_eq!(back.overridden().count(), 2);
    }

    #[test]
    fn clock_values_follow_the_zone_and_format() {
        let mut v = fresh();
        let t = civil_to_unix(2026, 7, 4, 3, 30, 15); // 03:30:15 UTC
        assert_eq!(v.get_at("QTIME", t).unwrap().text(), "033015");
        assert_eq!(v.get_at("QDATE", t).unwrap().text(), "070426");
        assert_eq!(v.get_at("QUTCOFFSET", t).unwrap().text(), "+0000");
        v.set_text("QTIMZON", "QN0500EST2").unwrap();
        v.set_text("QDATFMT", "YMD").unwrap();
        assert_eq!(v.get_at("QTIME", t).unwrap().text(), "233015"); // previous evening EDT
        assert_eq!(v.get_at("QDATE", t).unwrap().text(), "260703");
        assert_eq!(v.get_at("QDAYOFWEEK", t).unwrap().text(), "*FRI");
        assert_eq!(v.get_at("QUTCOFFSET", t).unwrap().text(), "-0400");
        assert_eq!(v.get_at("QCENTURY", t).unwrap().text(), "1");
        assert!(matches!(
            v.set_text("QDATE", "010101"),
            Err(SysValError::ReadOnly(_))
        ));
    }

    #[test]
    fn system_set_values() {
        let host = HostInfo {
            product_uuid: Some("12345678-1234-1234-1234-123456789abc".into()),
        };
        let mut v = SysVals::from_record(&Record::new(0), &host).unwrap();
        let serial = v.get("QSRLNBR").unwrap().text();
        assert_eq!(serial.len(), 7);
        assert_eq!(
            serial,
            serial_from_uuid("12345678-1234-1234-1234-123456789abc")
        );
        v.set_system("QABNORMSW", Value::Char("1".into())).unwrap();
        assert_eq!(v.get("QABNORMSW").unwrap().text(), "1");
        assert!(v.set_system("QABNORMSW", Value::Char("9".into())).is_err());
    }
}
