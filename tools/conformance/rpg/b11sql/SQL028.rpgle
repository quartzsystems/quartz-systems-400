**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) COMMIT(*NONE) OBJTYPE(*MODULE) OUTPUT(*PRINT)
//; run: no
//; sqlfile: EMPT
ctl-opt nomain;
// OBJTYPE(*MODULE): an exported procedure with SQL.
dcl-proc countDept export;
  dcl-pi *n int(10);
    dp char(3) const;
  end-pi;
  dcl-s n int(10);
  exec sql select count(*) into :n from EMPT where DEPT = :dp;
  return n;
end-proc;
