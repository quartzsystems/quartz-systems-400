/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SNDBRKMSG: send an immediate message that breaks in at work */
/* station message queues.                                      */
             CMD        PROMPT('Send Break Message') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(SNDBRKMSG)
             PARM       KWD(MSG) TYPE(*CHAR) LEN(512) MIN(1) +
                          PROMPT('Message text')
             PARM       KWD(TOMSGQ) TYPE(QM) MIN(1) MAX(50) +
                          SNGVAL((*ALLWS)) +
                          PROMPT('To work station message queue')
             PARM       KWD(MSGTYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*INFO) VALUES(*INFO *INQ) +
                          PROMPT('Message type')
             PARM       KWD(RPYMSGQ) TYPE(Q1) PMTCTL(P1) +
                          PROMPT('Message queue to get reply')
             Q1:        QUAL       TYPE(*NAME) LEN(10) DFT(QSYSOPR)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             P1:        PMTCTL     CTL(MSGTYPE) COND((*EQ *INQ))
             QM:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
