**FREE
//; dds: CUSTF
//; with: OA002S
//; setup: CRTRPGMOD MODULE({lib}/OA002S) SRCFILE({lib}/QRPGLESRC) SRCMBR(OA002S) DBGVIEW(*NONE)
//; setup: CRTSRVPGM SRVPGM({lib}/OA002S) MODULE({lib}/OA002S) EXPORT(*ALL)
//; cleanup: DLTSRVPGM SRVPGM({lib}/OA002S)
//; cleanup: DLTMOD MODULE({lib}/OA002S)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds log qualified inz;
  n int(10);
  l char(100) dim(40);
end-ds;
dcl-f CUSTF usage(*input:*output) handler('OA002S(NVHDL)' : log);
dcl-s i int(10);
read CUSTF;
dow not %eof(CUSTF);
  line = 'read ' + %char(CUSNO) + ' ' + %trimr(NAME) + ' ' + %trimr(CITY) + ' ' + %char(BAL);
  write QSYSPRT line;
  read CUSTF;
enddo;
CUSNO = 55;
NAME = 'Zed';
CITY = 'Bonn';
BAL = 12.5;
write CUSTR;
close CUSTF;
for i = 1 to log.n;
  line = log.l(i);
  write QSYSPRT line;
endfor;
*inlr = *on;
