**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// The callee of FCWHERE: fails without a handler of its own.
ctl-opt dftactgrp(*no) actgrp(*caller) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE'); name char(10); line char(80); end-pr;
dcl-s z packed(3:0);
z = z / 0;
QORACLE('FCWHERE' : 'callee continued');
*inlr = *on;
