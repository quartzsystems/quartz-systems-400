**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f SPCL special(20) usage(*input) pgmname('COVNOSPCL') usropn;
dcl-ds rec len(20) end-ds;
open(e) SPCL;
line = 'open ' + %char(%status(SPCL)) + ' ' + %error;
write QSYSPRT line;
*inlr = *on;
