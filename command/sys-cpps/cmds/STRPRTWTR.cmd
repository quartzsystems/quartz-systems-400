/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRPRTWTR: start a printer writer on a device. */
             CMD        PROMPT('Start Printer Writer') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSPCMD) HLPID(STRPRTWTR)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*ALL)) PROMPT('Printer')
             PARM       KWD(OUTQ) TYPE(Q1) DFT(*DEV) SNGVAL((*DEV)) +
                          PROMPT('Output queue')
             PARM       KWD(FORMTYPE) TYPE(E1) PROMPT('Form type options')
             PARM       KWD(FILESEP) TYPE(*INT2) DFT(*FILE) RANGE(0 9) +
                          SPCVAL((*FILE -1)) PROMPT('File separators')
             PARM       KWD(DRAWER) TYPE(*INT2) DFT(*DEVD) RANGE(1 255) +
                          SPCVAL((*DEVD 0) (*FILE -1)) +
                          PROMPT('Drawer for separators')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Writer options')
             PARM       KWD(WTR) TYPE(*NAME) LEN(10) DFT(*DEV) +
                          SPCVAL((*DEV)) PMTCTL(*PMTRQS) PROMPT('Writer')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*DEVD) +
                          SNGVAL((*DEVD) (*REQUESTER)) PMTCTL(*PMTRQS) +
                          PROMPT('Message queue')
             PARM       KWD(AUTOEND) TYPE(E2) DFT(*NO) SNGVAL((*NO)) +
                          PMTCTL(*PMTRQS) PROMPT('Automatically end writer')
             PARM       KWD(ALIGN) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*WTR) VALUES(*WTR *FILE *FIRST) +
                          PMTCTL(*PMTRQS) PROMPT('Forms alignment')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*STD) (*FORMS)) +
                          PROMPT('Form type')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*INQMSG) VALUES(*INQMSG *MSG *NOMSG +
                          *INFOMSG) PROMPT('Message option')
             E2:        ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('End writer')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NORDYF) VALUES(*NORDYF *FILEEND) +
                          PROMPT('When to end')
