**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
//; run: no
ctl-opt dftactgrp(*no);
dcl-s id int(10);
dcl-s n int(10);
dcl-s ind int(5) dim(3);
// What the precompiler refuses: FETCH ABSOLUTE, options of routines only,
// an indicator array element, a list of values not in parentheses.
exec sql set option commit = *none, dbgview = *none, output = *print, tgtrls = *current;
exec sql declare c1 scroll cursor for select ID from EMPT order by ID;
exec sql open c1;
exec sql fetch absolute 2 from c1 into :id;
exec sql select BONUS into :n :ind(2) from EMPT where ID = 2;
exec sql values 1, 2 into :id, :n;
*inlr = *on;
