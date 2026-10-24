# RPG open questions

Semantics the oracle has not settled yet, each with what QS/400 does
meanwhile and the corpus members that bear on it.

## Floats with decimal operands

`float op packed` on pub400 does not give the IEEE result of the two
operands converted to the nearest double. The model QS/400 uses: a
decimal operand of a float operation is converted to the double
toward zero from it, and a double assigned to a decimal is its exact
binary value truncated to the target's decimals (the RPG descriptor bit,
`ABI.md`). It matches 189 of the 200 cases of `EXP008`, `EXP018`,
`EXP028`, `EXP038` and `EXP048`; the other 11 differ in the last
decimal. The conversion of a float field's `INZ` literal (nearest, at
present) is the next suspect.

## Precision

The intermediate precision rules (`DECIMAL.md` §4) match every case of
the decimal and integer members of batch 3 (`EXP001`-`EXP060` but the
float ones), including the 63-digit members. `RNF7551` (the 63-digit
cap took decimals below the target's) and `RNF7057` (`EVAL(H)` with
nothing to round: the value's decimals are no more than the target's)
are reported where pub400 reports them, and so is every `RNF7551` of
the 100 fuzz bundles `FZB000`-`FZB099` (the rule and its line,
`RPG.md`, Numbers).

One case does not fit: `EXP114`'s `zoned(60:11) / packed(26:5)` into a
`packed(59:4)` keeps 9 decimals (the listing says so too) and pub400
gives `RNF7551`, though 9 is more than the target's 4. Every other
63-digit division in batch 3 that keeps at least the floor has no
message, among them `packed(43:0) / packed(14:11)` (9 decimals, floor
3), `packed(59:8) / packed(8:7)` (5, floor 3) and `zoned(60:8) /
packed(30:3)` (8, floor 3). One reading fits them all: a 63-digit
quotient is flagged when the dividend's digits plus the floor exceed 63
(60 + 4 here; at most 63 in the others). `EXP126` tests it (dividends
of 58 to 63 digits, floors 0 to 4, zoned and packed, a 31-digit
case) and `EXP127` the unsigned `%ABS` (`RPG.md`, Numbers); neither has
an outcome yet, the oracle's `CPYSPLF` failing for every member while
they were tried.
