**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 13: what the caller of a cancelled program receives.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE'); name char(10); line char(80); end-pr;
dcl-pr FCCALLEE extpgm('FCCALLEE'); end-pr;
dcl-ds psds psds;
  excid char(7) pos(40);
end-ds;
// Reply C to the inquiry; the caller's handler reports the message.
monitor;
  FCCALLEE();
on-error;
  QORACLE('CANCELTO' : 'caller got ' + excid + ' status ' + %char(%status));
endmon;
*inlr = *on;
