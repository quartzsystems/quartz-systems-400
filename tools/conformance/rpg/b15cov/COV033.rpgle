**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds info qualified;
  name varchar(20);
  qty int(10);
  price packed(7:2);
end-ds;
dcl-s doc varchar(200);
doc = '<info><name>Bolt</name><qty>12</qty><price>3.50</price></info>';
xml-into info %xml(doc:'doc=string');
line = info.name + ' ' + %char(info.qty) + ' ' + %editc(info.price:'X');
write QSYSPRT line;
doc = '<INFO><NAME>Nut</NAME><QTY>7</QTY><PRICE>0.25</PRICE></INFO>';
xml-into info %xml(doc:'doc=string case=upper');
line = info.name + ' ' + %char(info.qty) + ' ' + %editc(info.price:'X');
write QSYSPRT line;
*inlr = *on;
