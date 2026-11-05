**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*CHG) OUTPUT(*PRINT)
//; sqlfile: EMPT
//; post: CRTJRNRCV JRNRCV({lib}/QSRCV1)
//; post: CRTJRN JRN({lib}/QSJRN) JRNRCV({lib}/QSRCV1)
//; post: STRJRNPF FILE({lib}/EMPT) JRN({lib}/QSJRN) IMAGES(*BOTH)
//; cleanup: ENDJRNPF FILE({lib}/EMPT) JRN({lib}/QSJRN)
//; cleanup: DLTJRN JRN({lib}/QSJRN)
//; cleanup: DLTJRNRCV JRNRCV({lib}/QSRCV1) DLTOPT(*IGNINQMSG)
ctl-opt dftactgrp(*no) actgrp(*new);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s id int(10);
dcl-s nm varchar(20);
// SCROLL cursors: every orientation, past either end, WITH HOLD across COMMIT.
exec sql declare c1 scroll cursor with hold for select ID, NAME from EMPT order by ID;
exec sql declare c2 cursor for select ID, NAME from EMPT order by ID;
exec sql declare c3 insensitive scroll cursor for select ID from EMPT order by ID;
exec sql declare c4 sensitive scroll cursor for select ID from EMPT order by ID;
exec sql declare c5 sensitive scroll cursor for select ID, NAME from EMPT where ID > 0;
exec sql declare c6 asensitive no scroll cursor without hold for select ID from EMPT;
exec sql open c1;
show('open');
exec sql fetch first from c1 into :id, :nm;
show('first');
exec sql fetch last from c1 into :id, :nm;
show('last');
exec sql fetch prior from c1 into :id, :nm;
show('prior');
exec sql fetch relative -2 from c1 into :id, :nm;
show('rel -2');
exec sql fetch relative 2 from c1 into :id, :nm;
show('rel 2');
exec sql fetch current from c1 into :id, :nm;
show('current');
exec sql fetch relative -10 from c1 into :id, :nm;
show('rel -10');
exec sql fetch current from c1 into :id, :nm;
show('current');
exec sql fetch next from c1 into :id, :nm;
show('next');
exec sql fetch after from c1;
show('after');
exec sql fetch prior from c1 into :id, :nm;
show('prior');
exec sql fetch relative 0 from c1 into :id, :nm;
show('rel 0');
exec sql fetch relative -1 from c1 into :id, :nm;
show('rel -1');
exec sql fetch relative 99 from c1 into :id, :nm;
show('rel 99');
exec sql fetch before from c1;
show('before');
exec sql fetch next from c1 into :id, :nm;
show('next');
exec sql open c2;
exec sql fetch c2 into :id, :nm;
show('c2 next');
exec sql commit;
show('commit');
exec sql fetch next from c1 into :id, :nm;
show('c1 held');
exec sql fetch c2 into :id, :nm;
show('c2 closed');
exec sql close c1;
exec sql open c3;
exec sql fetch last from c3 into :id;
show('c3 last');
exec sql close c3;
exec sql open c4;
show('c4 open');
exec sql fetch first from c4 into :id;
show('c4 first');
exec sql close c4;
exec sql open c5;
show('c5 open');
exec sql fetch first from c5 into :id, :nm;
show('c5 first');
exec sql update EMPT set NAME = 'Zed' where ID = 2;
exec sql fetch next from c5 into :id, :nm;
show('c5 next');
exec sql close c5;
exec sql open c6;
exec sql fetch c6 into :id;
show('c6 next');
exec sql close c6;
exec sql rollback;
*inlr = *on;

// The statement's outcome.
dcl-proc show;
  dcl-pi *n;
    what varchar(20) const;
  end-pi;
  line = what + ' ' + %char(sqlcode) + ' ' + sqlstate + ' ' + %char(id) + ' ' + nm;
  write QSYSPRT line;
  id = 0;
  nm = '';
end-proc;
