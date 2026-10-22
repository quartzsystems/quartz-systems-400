/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CLRMSGQ: remove the messages on a message queue. */
             CMD        PROMPT('Clear Message Queue') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(CLRMSGQ)
             PARM       KWD(MSGQ) TYPE(Q1) MIN(1) PROMPT('Message queue')
             PARM       KWD(CLEAR) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *KEEPUNANS) +
                          PROMPT('Messages to clear')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*WRKSTN) (*USRPRF))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
