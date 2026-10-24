**FREE
//; dds: BENCHF
//; sqlfile: BENCHF
// 100,000 keyed CHAINs over 10,000 records, timed by the program.
ctl-opt dftactgrp(*no);
dcl-f BENCHF keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s i int(10);
dcl-s found int(10);
dcl-s k packed(7:0);
dcl-s sum packed(15:2);
dcl-s t0 timestamp;
dcl-s ms int(20);
t0 = %timestamp();
for i = 1 to 100000;
  k = %rem(i * 7919 : 10000);
  chain k BENCHR;
  if %found(BENCHF);
    found += 1;
    sum += BAMT;
  endif;
endfor;
ms = %diff(%timestamp() : t0 : *mseconds) / 1000;
line = 'chain ms ' + %char(ms) + ' found ' + %char(found) + ' ' + %editc(sum : 'X');
write QSYSPRT line;
*inlr = *on;
