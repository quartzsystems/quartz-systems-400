**FREE
//; dds: CUSTF
//; with: OA001H
//; setup: CRTBNDRPG PGM({lib}/OA001H) SRCFILE({lib}/QRPGLESRC) SRCMBR(OA001H) DBGVIEW(*NONE)
//; cleanup: DLTPGM PGM({lib}/OA001H)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds log qualified inz;
  n int(10);
  l char(100) dim(40);
end-ds;
dcl-f CUSTF keyed usage(*update:*output:*delete) handler('OA001H' : log) block(*no);
dcl-ds r likerec(CUSTR:*ALL);
dcl-s i int(10);
read CUSTF r;
dow not %eof(CUSTF);
  line = 'read ' + %char(r.CUSNO) + ' ' + %trimr(r.NAME) + ' ' + %trimr(r.CITY) + ' '
  + %char(r.BAL);
  write QSYSPRT line;
  read CUSTF r;
enddo;
chain (2) CUSTF r;
line = 'chain 2 ' + %found(CUSTF) + ' ' + %trimr(r.NAME);
write QSYSPRT line;
r.BAL = 77.5;
update CUSTR r;
chain (9) CUSTF r;
line = 'chain 9 ' + %found(CUSTF);
write QSYSPRT line;
r.CUSNO = 4;
r.NAME = 'Dora';
r.CITY = 'Oslo';
r.BAL = -1;
write CUSTR r;
setll (3) CUSTF;
line = 'setll 3 ' + %equal(CUSTF) + ' ' + %found(CUSTF);
write QSYSPRT line;
read CUSTF r;
line = 'next ' + %char(r.CUSNO) + ' ' + %trimr(r.NAME);
write QSYSPRT line;
delete (1) CUSTF;
line = 'delete ' + %found(CUSTF);
write QSYSPRT line;
chain(e) (8) CUSTF r;
line = 'err ' + %char(%status(CUSTF)) + ' ' + %error;
write QSYSPRT line;
monitor;
  chain (8) CUSTF r;
on-error;
  line = 'err ' + %char(%status(CUSTF)) + ' ' + %char(%status);
endmon;
write QSYSPRT line;
close CUSTF;
line = 'open ' + %open(CUSTF);
write QSYSPRT line;
for i = 1 to log.n;
  line = log.l(i);
  write QSYSPRT line;
endfor;
*inlr = *on;
