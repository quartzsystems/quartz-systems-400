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

## 6. Edit codes

`edit_code(value, shape, code, opts)` gives the characters `%EDITC` and
`EDTCDE` produce; `code_width` gives their count, which depends only on
the shape, the code and the fill.

| Codes | Commas | Zero balance | Negative |
|---|---|---|---|
| `1` `2` | yes | `1` prints, `2` blanks | no sign |
| `3` `4` | no | `3` prints, `4` blanks | no sign |
| `A` `B` | yes | `A` prints, `B` blanks | `CR` after |
| `C` `D` | no | `C` prints, `D` blanks | `CR` after |
| `J` `K` | yes | `J` prints, `K` blanks | `-` after |
| `L` `M` | no | `L` prints, `M` blanks | `-` after |
| `N` `O` | yes | `N` prints, `O` blanks | `-` before, floating |
| `P` `Q` | no | `P` prints, `Q` blanks | `-` before, floating |

Rules: leading zeros of the integer part are blanks, and so is a comma
in that region. The decimal point and every decimal digit always print,
so a value below one prints as `.05` and a zero balance as `.00`; a
field without decimals prints its units digit, so a zero balance is `0`.
`CR` and `-` after the number are blanks when it is positive. Width is
digits, plus one comma per three integer digits beyond the first, plus
the point, plus the sign, plus one for a currency symbol. `*ASTFILL`
puts `*` in every suppressed position; `*CURSYM` puts the symbol just
left of the first printed character, and a floating minus goes left of
that. A blank-balance code prints blanks for zero whatever the fill.

`X` prints the digits with their leading zeros and no sign. `Z` prints
the digits with leading zeros blank, no sign and no point; zero prints
blank. `Y` prints a date: the first zero blank, separators between the
groups, which for 3 to 9 digits are `n/nn`, `nn/nn`, `n/nn/nn`,
`nn/nn/nn`, `nnn/nn/nn`, `nn/nn/nnnn` (`nnnn/nn/nn` under `*YMD`) and
`nnn/nn/nnnn` (`nnnnn/nn/nn` under `*YMD`); format and separator come
from `DATEDIT`.

## 7. Edit words

`edit_word(value, shape, word)` gives the characters `%EDITW` and
`EDTWRD` produce; the result is as long as the word.

The body runs from the first character through the digit position that
takes the last digit. A blank, `0` or `*` is a digit position; the first
`0` is the zero-suppression stop: the digit there and every digit to its
right print even when zero. Before the first significant digit (and
before the stop), digit positions and insertion characters print the
fill, which is `*` when any digit position is an asterisk and a blank
otherwise. `&` prints a blank wherever it is. A `$` as the first
character is a fixed currency symbol and prints as is; a `$` just left
of the stop floats: its position is given to the digits and the symbol
prints just left of the first printed digit. Other characters are
insertion characters and print once a digit has printed, or at and
after the stop.

The status follows the body: `CR` or `-`, optionally after `&` blanks,
printed for a negative value and blank otherwise. Everything after the
status is the expansion and prints as is, `&` as blank.

## 8. The oracle

`qdfuzz kit DIR` writes the differential fuzzer kit to `DIR`:
`cases.csv`, our `expected.csv`, the RPG program `QDFUZZ.RPGLE` and the
DDS of its files, and a README with the commands to run on an IBM i.
The cases cover every pair of eight operand shapes with every operation
and chosen edge values, every edit code with every fill on three shapes,
fifteen edit words, and seeded random operands; the program has one
`when` per shape pair so every operand carries the declared precision.
`qdfuzz diff cases.csv results.csv` compares the machine's rows; the
test `qdecimal/tests/oracle.rs` does the same and is skipped while
`conformance/oracle/qdecimal/results.csv` is absent. Numbers compare by
value and scale, so `.5` and `0.5` agree; edited text compares exactly.
