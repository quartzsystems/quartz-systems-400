/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTMSGQ: delete one or more message queues. */
             CMD        PROMPT('Delete Message Queue') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(DLTMSGQ)
             PARM       KWD(MSGQ) TYPE(Q1) MIN(1) PROMPT('Message queue')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALL) +
                          (*ALLUSR)) PROMPT('Library')
