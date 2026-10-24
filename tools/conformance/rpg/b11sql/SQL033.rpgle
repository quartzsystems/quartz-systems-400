**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; run: no
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s nm varchar(20);
exec sql select NAME into :nm from EMPT where ID = 1;
nm = u1 + nm;
*inlr = *on;
dcl-proc p1;
  dcl-s k int(10);
  dcl-s r varchar(20);
  k = 3;
  exec sql select NAME into :r from EMPT where ID = :k;
  r = u2;
  exec sql update EMPT set NAME = :r where ID = :k;
  r = u3;
end-proc;
