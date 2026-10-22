/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRRMTWTR: start a writer that sends an output queue to a remote */
/* system.                                                           */
             CMD        PROMPT('Start Remote Writer') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSPCMD) HLPID(STRRMTWTR)
             PARM       KWD(OUTQ) TYPE(Q1) MIN(1) PROMPT('Output queue')
             PARM       KWD(MSGQ) TYPE(Q2) DFT(*REQUESTER) +
                          SNGVAL((*REQUESTER) (*NONE)) +
                          PROMPT('Message queue')
             PARM       KWD(FORMTYPE) TYPE(E1) PROMPT('Form type options')
             PARM       KWD(WTR) TYPE(*NAME) LEN(10) DFT(*OUTQ) +
                          SPCVAL((*OUTQ)) PROMPT('Writer')
             PARM       KWD(AUTOEND) TYPE(E2) DFT(*NO) SNGVAL((*NO)) +
                          PROMPT('Automatically end writer')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*STD) (*FORMS)) +
                          PROMPT('Form type')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NOMSG) VALUES(*NOMSG *INQMSG *MSG +
                          *INFOMSG) PROMPT('Message option')
             E2:        ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('End writer')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NORDYF) VALUES(*NORDYF *FILEEND) +
                          PROMPT('When to end')
