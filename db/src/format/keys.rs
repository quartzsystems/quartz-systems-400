// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sizes of the encoded key parts. See `docs/DB_FORMAT.md` §6; the
//! encoding itself arrives with the access path checkpoint.

use dds::{DataType, FieldDef, KeyTransform};

/// Bytes a key field contributes to the encoded key.
#[must_use]
pub fn part_len(field: &FieldDef, _transform: KeyTransform) -> usize {
    let null = usize::from(field.allow_null);
    let data = match field.data_type {
        DataType::Char | DataType::Hex | DataType::Graphic | DataType::DbcsOpen => {
            // The data without the VARLEN prefix.
            usize::try_from(field.data_len()).expect("fits")
        }
        DataType::Zoned | DataType::Packed | DataType::Binary => {
            1 + usize::try_from(field.length).expect("fits")
        }
        DataType::Float | DataType::Date => 8,
        DataType::Time => 6,
        DataType::Timestamp => 26,
    };
    null + data
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsname::Name;

    fn f(t: DataType, len: u32) -> FieldDef {
        FieldDef::new(Name::parse("X").unwrap(), t, len, 0)
    }

    #[test]
    fn part_lengths() {
        assert_eq!(part_len(&f(DataType::Char, 30), KeyTransform::Unsigned), 30);
        assert_eq!(part_len(&f(DataType::Packed, 7), KeyTransform::Signed), 8);
        assert_eq!(part_len(&f(DataType::Zoned, 9), KeyTransform::Absval), 10);
        assert_eq!(part_len(&f(DataType::Binary, 4), KeyTransform::Signed), 5);
        assert_eq!(part_len(&f(DataType::Float, 9), KeyTransform::Signed), 8);
        assert_eq!(part_len(&f(DataType::Date, 10), KeyTransform::Unsigned), 8);
        assert_eq!(part_len(&f(DataType::Time, 8), KeyTransform::Unsigned), 6);
        assert_eq!(
            part_len(&f(DataType::Timestamp, 26), KeyTransform::Unsigned),
            26
        );
        let mut n = f(DataType::Char, 5);
        n.allow_null = true;
        assert_eq!(part_len(&n, KeyTransform::Unsigned), 6);
    }
}
