**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f CUSTF keyed usage(*update:*output:*delete) block(*no);
dcl-ds r likerec(CUSTR:*ALL);
bump(CUSTF : 104 : 10.5);
bump(CUSTF : 199 : 1);
add(CUSTF : 120 : 'Ivy Irwin');
drop(CUSTF : 101);
setll *loval CUSTF;
read CUSTF r;
dow not %eof(CUSTF);
  line = %char(r.CUSNO) + ' ' + %trimr(r.NAME) + ' ' + %char(r.BAL);
  write QSYSPRT line;
  read CUSTF r;
enddo;
*inlr = *on;

dcl-proc bump;
  dcl-pi *n;
    f likefile(CUSTF);
    key packed(5) value;
    amt packed(9:2) value;
  end-pi;
  dcl-ds x likerec(CUSTR:*ALL);
  chain key f x;
  if %found(f);
    x.BAL += amt;
    update f.CUSTR x;
    line = 'bumped ' + %char(key);
  else;
    line = 'no ' + %char(key) + ' ' + %found(f);
  endif;
  write QSYSPRT line;
end-proc;

dcl-proc add;
  dcl-pi *n;
    f likefile(CUSTF);
    key packed(5) value;
    nm char(20) const;
  end-pi;
  dcl-ds x likerec(CUSTR:*OUTPUT);
  x.CUSNO = key;
  x.NAME = nm;
  x.CITY = 'Reno';
  x.BAL = 1;
  write f.CUSTR x;
end-proc;

dcl-proc drop;
  dcl-pi *n;
    f likefile(CUSTF);
    key packed(5) value;
  end-pi;
  delete (key) f;
  line = 'dropped ' + %char(key) + ' ' + %found(f);
  write QSYSPRT line;
end-proc;
