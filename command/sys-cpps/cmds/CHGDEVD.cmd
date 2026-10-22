/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGDEVD: change the attributes common to every device description  */
/* (QS/400 keeps one command where IBM has one per device category).   */
             CMD        PROMPT('Change Device Description') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHCFCMD) HLPID(CHGDEVD)
             PARM       KWD(DEVD) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Device description')
             PARM       KWD(ONLINE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Online at IPL')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*SYSOPR)) PROMPT('Message queue')
             PARM       KWD(PRTDEV) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*SYSVAL)) PMTCTL(*PMTRQS) +
                          PROMPT('Print device')
             PARM       KWD(OUTQ) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*DEV)) PMTCTL(*PMTRQS) +
                          PROMPT('Output queue')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Description')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
