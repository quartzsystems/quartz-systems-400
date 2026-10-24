**FREE
//; dds: B8A
//; sql: CREATE TABLE {lib}.COVT126 (A CHAR(1) CCSID 37)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f B8A workstn usropn maxdev(*file) devid(dev);
dcl-s dev char(10) inz('none');
open(e) B8A;
line = 'open ' + %char(%status(B8A)) + ' ' + %error;
write QSYSPRT line;
acq(e) 'COVNODEV' B8A;
line = 'acq ' + %char(%status(B8A)) + ' ' + %error + ' ' + dev;
write QSYSPRT line;
next(e) 'COVNODEV' B8A;
line = 'next ' + %char(%status(B8A)) + ' ' + %error;
write QSYSPRT line;
rel(e) 'COVNODEV' B8A;
line = 'rel ' + %char(%status(B8A)) + ' ' + %error;
write QSYSPRT line;
*inlr = *on;
