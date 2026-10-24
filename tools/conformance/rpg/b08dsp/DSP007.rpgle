**FREE
//; dds: B8D
//; keys: @3,12 enter
//; keys: @5,20 f5
//; keys: @1,1 f10
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8D workstn infds(fb);
dcl-ds fb;
  fbfmt char(8) pos(38);
  aid uns(3) pos(369);
  crow uns(3) pos(370);
  ccol uns(3) pos(371);
end-ds;
// The display feedback: format, attention identifier, cursor.
RES = 'Start';
dou *in03;
exfmt KEYS;
RES = 'fmt=' + %trim(fbfmt) + ' aid=' + %char(aid) + ' row=' + %char(crow) + ' col=' + %char(ccol);
enddo;
*inlr = *on;
