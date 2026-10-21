**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 15: QCMDEXC runs the command at once.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE'); name char(10); line char(80); end-pr;
dcl-pr QCMDEXC extpgm('QCMDEXC'); cmd char(200) const; len packed(15:5) const; end-pr;
dcl-pr QMHRCVPM extpgm('QMHRCVPM'); recv char(300); recvlen int(10) const; fmt char(8) const;
  entry char(10) const; counter int(10) const; type char(10) const; key char(4) const;
  wait int(10) const; action char(10) const; err char(16); end-pr;
dcl-s recv char(300);
dcl-s err char(16) inz(x'00000010');
dcl-s cmd char(200);
cmd = 'SNDPGMMSG MSG(''from QCMDEXC'') TOPGMQ(*PRV)';
QCMDEXC(cmd : 200);
QMHRCVPM(recv : 300 : 'RCVM0100' : '*' : 0 : '*ANY' : x'00000000' : 0 : '*SAME' : err);
QORACLE('CMDEXC' : 'received [' + %subst(recv : 57 : %int(%subst(recv : 49 : 4))) + ']');
*inlr = *on;
