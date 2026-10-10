# Decimal arithmetic

How QS/400 holds and computes decimal numbers. The `qdecimal` crate
implements this document; where IBM i behaviour is uncertain the choice
is listed in `DB_OPEN_QUESTIONS.md` and the differential fuzzer against
the real machine settles it.

## 1. The value

A `Decimal` is a sign, a magnitude of at most 63 decimal digits and a
scale of 0 to 63: `(-1)^neg * magnitude / 10^scale`. Zero has no sign.
Two values compare numerically whatever their scales (`1.0 == 1.00`);
the scale is kept so that a value remembers the decimal positions it
was declared with.

A `Shape` is the declared `digits,scale` of a field or literal: 1 to 63
digits, scale at most digits. `integer = digits - scale`.

## 2. Encodings

All multi-byte integers are big-endian, as on the machine.

**Packed** (`P digits,scale`): `digits / 2 + 1` bytes. Digits occupy
the nibbles from the left, right-aligned so the last digit sits in the
high nibble of the last byte; the low nibble of the last byte is the
sign. When `digits` is even the first nibble is a pad and must be zero.
Written: `C` positive, `D` negative, zero is `+0`. Read: `A C E F` are
positive, `B D` negative; any other sign nibble, a digit nibble above 9
or a nonzero pad is a decimal data error (MCH1202).

**Zoned** (`S digits,scale`): one byte per digit, zone `F` and the digit
in the low nibble; the sign is the zone of the last byte, `C` or `D` as
written. Read: zones before the last byte must be `F`; the last zone
takes `A C E F` as positive and `B D` as negative; anything else, or a
digit above 9, is MCH1202. Blanks (`0x40`) are therefore an error.

**Binary** (`B digits,scale`, digits 1 to 18): a two's complement
integer with `scale` implied decimals; 2 bytes up to 4 digits, 4 bytes
up to 9, 8 bytes beyond. Writing a value with more integer digits than
declared is a size error; reading returns the value held even when it
exceeds the declared digits (a `B 4,0` can hold 32767).

**Integer** (`I`/`U` 1, 2, 4 or 8 bytes): two's complement or unsigned.
Writing takes the integer part toward zero and checks the range.

**Float** (`F`, `FLTPCN(*SINGLE)` 4 bytes, `*DOUBLE` 8): IEEE 754.
Writing takes the nearest float; reading gives the shortest decimal
that reads back as the same float. NaN and infinity cannot become a
decimal.

## 3. Assignment

Assigning a value to a shape first sets the scale: zeros are added, or
digits dropped by **truncation** or **half-adjust** (the first dropped
digit decides: 5 or more adds one to the last kept digit, away from
zero). Then the integer part must fit `digits - scale` positions, or
the assignment fails with a **size error** (MCH1210, which the RPG
runtime will report as RNQ0103 / status 00103). Fixed-form RPG
operations drop high-order digits instead; `fit_truncate` does that.

## 4. Intermediate results

Arithmetic takes typed operands, a value with its declared shape, and
yields a typed result; the result's shape follows the rules below and
the exact result is truncated (never rounded) to it. Literals have the
shape of their own digits.

| Operation | Integer digits | Decimal positions |
|---|---|---|
| `a + b`, `a - b` | `max(i1, i2) + 1` | `max(d1, d2)` |
| `a * b` | `i1 + i2` | `d1 + d2` |
| `a / b` | `i1 + d2` | `63 - (i1 + d2)` |

When integer digits plus decimals exceed 63, decimal positions are
dropped first, down to `63 - integer`. When the integer digits alone
exceed 63 the operation fails with a size error. Division by zero is
MCH1211.

**`*RESDECPOS`** (`EXPROPTS(*RESDECPOS)` or the `R` extender): the same
rules, except that the decimal positions of an intermediate result are
never reduced below the result field's decimal positions. If that
leaves too few integer digits for the value, the operation fails with a
size error. The default rules and `*RESDECPOS` are a `Mode` on every
operation.

Examples (default rules):

- `5,2 + 7,3` gives `9,3`.
- `31,0 * 32,0` gives `63,0`; `40,0 * 40,0` is a size error.
- `40,20 * 40,20` gives `63,23`: 80 integer digits would be needed for
  40 decimals; the decimals give way to 23.
- `1,0 / 1,0` gives `63,62`, so `1 / 3` is `0.333...3` with 62 threes.
- `5,1 / 3,2` gives `6` integer digits and `57` decimals.

## 5. Errors

| Error | Message | Meaning |
|---|---|---|
| `DecimalData` | MCH1202 | bytes that are not valid packed or zoned data |
| `SizeOverflow` | MCH1210 | a value too large for its target or for an intermediate result |
| `DivideByZero` | MCH1211 | division by zero |
| `Precision` | none | a declaration the machine does not allow |
| `Parse` | none | text that is not a number |
| `NotFinite` | none | a NaN or infinite float |

## 6. Edit codes and words

`TODO`: `%EDITC` and `%EDITW` follow in the next part of this crate.
