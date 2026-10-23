**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed infds(fb);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds fb;
  fbfile *file;
  fbopen char(1) pos(9);
  fbeof char(1) pos(10);
  fbstat *status;
  fbop *opcode;
  fbrec *record;
  fbmsg char(7) pos(46);
  fbrrn int(10) pos(397);
end-ds;
// The file information data structure.
read CUSTF;
line = fbfile + '|' + fbopen + fbeof + '|' + %char(fbstat) + '|' + fbop + '|' + fbrec;
write QSYSPRT line;
line = 'rrn ' + %char(fbrrn) + ' msg ' + fbmsg + '|';
write QSYSPRT line;
chain (106) CUSTR;
line = fbop + '|' + fbrec + '|' + %char(fbstat) + ' rrn ' + %char(fbrrn);
write QSYSPRT line;
chain (199) CUSTF;
line = fbop + '|' + %char(fbstat) + '|' + fbeof;
write QSYSPRT line;
setgt *hival CUSTF;
read CUSTF;
line = fbop + '|' + %char(fbstat) + '|' + fbeof;
write QSYSPRT line;
*inlr = *on;
