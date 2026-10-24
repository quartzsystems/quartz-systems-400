**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s id int(10);
dcl-s nm varchar(20);
dcl-s k int(10) inz(-2);
// A scroll cursor in every direction.
exec sql declare c1 scroll cursor for select ID, NAME from EMPT order by ID;
exec sql open c1;
exec sql fetch last from c1 into :id, :nm;
line = 'last ' + %char(id) + ' ' + nm + ' ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql fetch prior from c1 into :id, :nm;
line = 'prior ' + %char(id) + ' ' + nm + ' ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql fetch first from c1 into :id, :nm;
line = 'first ' + %char(id) + ' ' + nm + ' ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql fetch relative 2 from c1 into :id, :nm;
line = 'rel 2 ' + %char(id) + ' ' + nm + ' ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql fetch relative :k from c1 into :id, :nm;
line = 'rel k ' + %char(id) + ' ' + nm + ' ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql fetch next from c1 into :id, :nm;
line = 'next ' + %char(id) + ' ' + nm + ' ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql fetch prior from c1 into :id, :nm;
line = 'prior ' + %char(id) + ' ' + nm + ' ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql fetch before from c1;
line = 'before ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql fetch next from c1 into :id, :nm;
line = 'next ' + %char(id) + ' ' + nm + ' ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql close c1;
*inlr = *on;
