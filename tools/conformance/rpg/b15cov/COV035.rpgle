**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds part qualified;
  id int(10);
  name varchar(20);
  note varchar(20) inz('unset');
end-ds;
xml-into part %xml('<part id="7"><name>  wheel  </name><color>red</color></part>'
  :'doc=string allowmissing=yes allowextra=yes');
line = %char(part.id) + ' [' + part.name + '] ' + part.note;
write QSYSPRT line;
xml-into part %xml('<part id="8"><name>  axle </name><note>n</note></part>':'doc=string trim=none');
line = %char(part.id) + ' [' + part.name + '] ' + part.note;
write QSYSPRT line;
*inlr = *on;
