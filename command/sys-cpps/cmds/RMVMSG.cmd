/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVMSG: remove one message or a group of them from a queue. */
             CMD        PROMPT('Remove Message') MAXPOS(*NONE) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(RMVMSG)
             PARM       KWD(PGMQ) TYPE(E1) +
                          PROMPT('Call stack entry message queue')
             PARM       KWD(MSGQ) TYPE(Q1) PROMPT('Message queue')
             PARM       KWD(MSGKEY) TYPE(*CHAR) LEN(4) +
                          PROMPT('Message key')
             PARM       KWD(CLEAR) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*BYKEY) VALUES(*BYKEY *ALL *KEEPUNANS +
                          *OLD *NEW) PROMPT('Messages to remove')
             Q1:        QUAL       TYPE(*NAME) LEN(10) DFT(*PGMQ) +
                          SPCVAL((*PGMQ) (*EXT) (*WRKSTN) (*USRPRF))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *PRV) +
                          PROMPT('Relationship')
                        ELEM       TYPE(Q2) PROMPT('Call stack entry')
             Q2:        QUAL       TYPE(*CHAR) LEN(10) DFT(*) +
                          SPCVAL((*) (*CTLBDY) (*PGMBDY) (*PGMNAME)) +
                          PROMPT('Call stack entry')
                        QUAL       TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Module')
                        QUAL       TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Bound program')
