**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// SQL in a subprocedure, with a parameter as a host variable.
line = getName(1) + ' ' + getName(5) + ' ' + getName(9);
write QSYSPRT line;
*inlr = *on;
dcl-proc getName;
  dcl-pi *n varchar(20);
    k int(10) const;
  end-pi;
  dcl-s r varchar(20);
  exec sql select NAME into :r from EMPT where ID = :k;
  if sqlcode <> 0;
    r = '?' + %char(sqlcode);
  endif;
  return r;
end-proc;
