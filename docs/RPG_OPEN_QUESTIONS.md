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
are reported where pub400 reports them.
