**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds item qualified dim(5);
  id int(10);
  text varchar(10);
end-ds;
dcl-ds sds psds;
  elems int(20) pos(372);
end-ds;
dcl-s i int(10);
dcl-s doc varchar(300);
doc = '<list><item><id>1</id><text>one</text></item>' + '<item><id>2</id><text>two</text></item>'
    + '<item><id>3</id><text>three</text></item></list>';
xml-into item %xml(doc:'doc=string path=list/item');
line = 'elements ' + %char(elems);
write QSYSPRT line;
for i = 1 to elems;
  line = %char(item(i).id) + ' ' + item(i).text;
  write QSYSPRT line;
endfor;
*inlr = *on;
