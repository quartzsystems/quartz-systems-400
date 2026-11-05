**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s d date;
dcl-s t time;
dcl-s nm varchar(20);
// SET OPTION: every option the listing shows, and their effect on the run.
exec sql set option commit = *none, naming = *sys, datfmt = *iso, datsep = '-',
  timfmt = *iso, timsep = '.', closqlcsr = *endmod, alwcpydta = *optimize,
  alwblk = *allread, dlyprp = *no, dynusrprf = *user, usrprf = *naming,
  srtseq = *hex, langid = *job, sqlcurrule = *db2, sqlpath = *libl,
  rdbcnnmth = *duw, decmpt = *period,
  decresult = (31, 31, 0), decfltrnd = *halfeven, conacc = *dft,
  optlob = *yes, extind = *no,
  dftrdbcol = *none, dyndftcol = *no, monitor = *user, sqlca = *yes;
exec sql select HIRED into :d from EMPT where ID = 1;
line = %char(sqlcode) + ' ' + %char(d);
write QSYSPRT line;
exec sql values current time into :t;
line = %char(sqlcode);
write QSYSPRT line;
exec sql select NAME into :nm from EMPT where ID = 2;
line = %char(sqlcode) + ' ' + nm;
write QSYSPRT line;
*inlr = *on;
