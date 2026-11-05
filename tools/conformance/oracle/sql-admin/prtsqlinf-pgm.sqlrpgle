**FREE
ctl-opt dftactgrp(*no);
dcl-s n int(10);
dcl-s nm varchar(20);
dcl-s stmt varchar(200);
exec sql set option commit = *none, naming = *sys, datfmt = *iso;
exec sql select count(*) into :n from sysibm/sysdummy1;
exec sql declare c1 cursor for
  select table_name from qsys2/systables where table_schema = :nm;
exec sql open c1;
exec sql fetch c1 into :nm;
exec sql close c1;
stmt = 'values 1';
exec sql prepare s1 from :stmt;
exec sql values current date into :nm;
*inlr = *on;
return;
