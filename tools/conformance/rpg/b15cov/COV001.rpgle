**FREE
//; dds: B8A
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f B8A workstn usropn maxdev(*file) devid(dev);
dcl-s dev char(10) inz('none');
acq(e) 'COVNODEV' B8A;
line = 'acq ' + %char(%status(B8A)) + ' ' + %error + ' ' + dev;
write QSYSPRT line;
monitor;
  acq 'COVNODEV' B8A;
  line = 'acq mon ok';
  write QSYSPRT line;
on-error;
  line = 'acq mon ' + %char(%status);
  write QSYSPRT line;
endmon;
*inlr = *on;
