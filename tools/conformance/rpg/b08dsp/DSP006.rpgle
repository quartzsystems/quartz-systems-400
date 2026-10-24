**FREE
//; dds: B8F
//; setup: CRTMSGF MSGF({lib}/B8MSGF)
//; setup: ADDMSGD MSGID(USR8001) MSGF({lib}/B8MSGF) MSG('The code must be OK')
//; cleanup: DLTMSGF MSGF({lib}/B8MSGF)
//; keys: 3,10=X enter
//; keys: reset 3,10=OK enter
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8F workstn;
// ERRMSGID: the message from a message file.
dou not *in52;
exfmt ENTRY;
*in52 = CODE <> 'OK';
enddo;
DONE = 'Code accepted';
exfmt ENTRY;
*inlr = *on;
