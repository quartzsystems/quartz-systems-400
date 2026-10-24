**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds grid qualified;
  cells zoned(2:0) dim(6) inz(5);
  row2 zoned(2:0) dim(3) pos(7);
end-ds;
dcl-s i int(10);
for i = 1 to 3;
  grid.row2(i) = i * 10;
endfor;
line = %char(grid.cells(1)) + ' ' + %char(grid.cells(4)) + ' ' + %char(grid.cells(6)) +
  ' ' + %char(%xfoot(%subarr(grid.cells : 4 : 3)));
write QSYSPRT line;
*inlr = *on;
