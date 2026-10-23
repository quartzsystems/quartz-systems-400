**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; dds: ORDF
//; sqlfile: ORDF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usropn;
dcl-f ORDF keyed usropn;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// OPEN and CLOSE several; *ALL; a subprocedure reads a global file.
dcl-pr count int(10);
end-pr;
open CUSTF;
open ORDF;
line = 'orders ' + %char(count()) + ' ' + %open(CUSTF) + %open(ORDF);
write QSYSPRT line;
close *all;
open QSYSPRT;
line = 'closed ' + %open(CUSTF) + %open(ORDF);
write QSYSPRT line;
*inlr = *on;
dcl-proc count;
  dcl-pi *n int(10);
  end-pi;
  dcl-s n int(10);
  read ORDF;
  dow not %eof(ORDF);
    n += 1;
    read ORDF;
  enddo;
  return n;
end-proc;
