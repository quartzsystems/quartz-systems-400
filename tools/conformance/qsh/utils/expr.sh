# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# expr: arithmetic, comparisons, |, &, :, groups, operand text, errors and statuses.

probe arith 'expr 1 + 2 \* 3; expr \( 1 + 2 \) \* 3; expr -17 / 5; expr -17 % 5'
probe compare 'expr 9 \< 10; expr a9 \< a10; expr 10 = 010; expr abc = abc'
probe or-and 'expr 0 \| 7; expr '\'''\'' \| x; expr 5 \& 0; echo $?'
probe match 'expr abcdef : abc; expr abcdef : bcd; expr hello.txt : '\''\(.*\)\.txt'\''; expr hello.c : '\''\(.*\)\.txt'\''; echo $?'
probe caret 'expr '\''^x'\'' : '\''^x'\''; expr x : '\''^x'\'''
probe operand-text 'expr 0042; expr -0; echo $?'
probe status 'expr 0; echo $?; expr '\'''\''; echo $?; expr 1; echo $?'
probe overflow 'expr 9223372036854775807 + 1; echo $?'
probe big-operand 'expr 99999999999999999999 + 1; echo $?'
probe div-zero 'expr 1 / 0; echo $?'
probe non-integer 'expr a + 1; echo $?; expr +5 + 1; echo $?'
probe syntax 'expr 1 +; echo $?; expr 1 2; echo $?; expr \( 1; echo $?; expr; echo $?'
probe operators-as-operands 'expr = = =; expr \(; expr \); expr -- 3 + 4'
probe bad-regex 'expr abc : '\''a\(b'\''; echo $?'
finish
