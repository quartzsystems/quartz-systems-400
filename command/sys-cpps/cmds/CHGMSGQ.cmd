/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGMSGQ: change a message queue's delivery mode and attributes. */
             CMD        PROMPT('Change Message Queue') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(CHGMSGQ)
             PARM       KWD(MSGQ) TYPE(Q1) MIN(1) PROMPT('Message queue')
             PARM       KWD(DLVRY) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *HOLD *BREAK *NOTIFY +
                          *DFT) PROMPT('Delivery')
             PARM       KWD(PGM) TYPE(Q2) DFT(*SAME) +
                          SNGVAL((*SAME) (*DSPMSG)) +
                          PROMPT('Break handling program')
             PARM       KWD(SEV) TYPE(*INT2) DFT(*SAME) RANGE(0 99) +
                          SPCVAL((*SAME -1)) PROMPT('Severity code filter')
             PARM       KWD(RESET) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Reset old messages')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Description')
             PARM       KWD(ALWALT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Allow alerts')
             PARM       KWD(MSGQFULL) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SNDRCV *NOWRAP +
                          *WRAP) PMTCTL(*PMTRQS) +
                          PROMPT('Queue full action')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*WRKSTN) (*USRPRF))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
