**FREE
//; dds: COVALI
//; sql: INSERT INTO {lib}.COVALI VALUES (1, 'first')
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds r extname('COVALI') alias qualified end-ds;
r.item_number = 9;
r.item_description = 'aliased';
line = %char(r.item_number) + ' ' + r.item_description;
write QSYSPRT line;
*inlr = *on;
