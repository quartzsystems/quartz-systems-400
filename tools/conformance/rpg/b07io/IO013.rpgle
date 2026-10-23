**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output) usropn;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// (E): %ERROR and %STATUS after file errors.
read(e) CUSTF;
line = 'closed ' + %error + ' ' + %char(%status) + ' ' + %char(%status(CUSTF));
write QSYSPRT line;
open CUSTF;
update(e) CUSTR;
line = 'no read ' + %error + ' ' + %char(%status(CUSTF));
write QSYSPRT line;
CUSNO = 101;
write(e) CUSTR;
line = 'dup ' + %error + ' ' + %char(%status(CUSTF));
write QSYSPRT line;
chain(e) (101) CUSTF;
line = 'chain ' + %error + ' ' + %char(%status(CUSTF)) + ' ' + %found(CUSTF);
write QSYSPRT line;
chain(e) (555) CUSTF;
line = 'nf ' + %error + ' ' + %char(%status(CUSTF)) + ' ' + %found(CUSTF);
write QSYSPRT line;
open(e) CUSTF;
line = 'reopen ' + %error + ' ' + %char(%status(CUSTF));
write QSYSPRT line;
close CUSTF;
*inlr = *on;
