**FREE
ctl-opt dftactgrp(*no) validate(*nodatetime) cvtopt(*nodatetime) ccsidcvt(*excp);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s d date inz(d'2026-10-24');
line = %char(d:*iso);
write QSYSPRT line;
*inlr = *on;
