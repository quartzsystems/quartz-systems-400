**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s #count int(10) inz(3);
dcl-s $amount packed(7:2) inz(1.5);
dcl-s @at char(3) inz('at');
dcl-s a_name_that_is_rather_long_for_an_rpg_variable_but_valid_x int(10) inz(77);
dcl-s name_with_digits_123 int(10) inz(123);
line = %char(#count) + ' ' + %char($amount) + ' ' + @at;
write QSYSPRT line;
line = %char(a_name_that_is_rather_long_for_an_rpg_variable_but_valid_x);
write QSYSPRT line;
line = %char(name_with_digits_123);
write QSYSPRT line;
*inlr = *on;
