**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f COVOA disk(20) usage(*input) handler('COVNOHDL') usropn;
dcl-ds rec len(20) end-ds;
open(e) COVOA;
line = 'open ' + %char(%status(COVOA)) + ' ' + %error;
write QSYSPRT line;
*inlr = *on;
