**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s dp char(3);
dcl-s n int(10);
dcl-s tot packed(11:2);
dcl-s av packed(11:2);
// GROUP BY through a cursor.
exec sql declare c1 cursor for select DEPT, count(*), sum(SAL), avg(SAL) from EMPT
  group by DEPT order by DEPT;
exec sql open c1;
dow 1 = 1;
  exec sql fetch c1 into :dp, :n, :tot, :av;
  if sqlcode <> 0;
    leave;
  endif;
  line = dp + ' ' + %char(n) + ' ' + %char(tot) + ' ' + %char(av);
  write QSYSPRT line;
enddo;
exec sql close c1;
*inlr = *on;
