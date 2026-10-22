/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SNDMSG: send an immediate message to users or message queues. */
             CMD        PROMPT('Send Message') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(SNDMSG)
             PARM       KWD(MSG) TYPE(*CHAR) LEN(512) MIN(1) +
                          PROMPT('Message text')
             PARM       KWD(TOUSR) TYPE(*NAME) LEN(10) +
                          SPCVAL((*SYSOPR) (*ALLACT) (*REQUESTER)) +
                          PROMPT('To user profile')
             PARM       KWD(TOMSGQ) TYPE(Q1) MAX(50) +
                          PROMPT('To message queue')
             PARM       KWD(MSGTYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*INFO) VALUES(*INFO *INQ) +
                          PROMPT('Message type')
             PARM       KWD(RPYMSGQ) TYPE(Q2) PMTCTL(P1) +
                          PROMPT('Message queue to get reply')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) DFT(*WRKSTN) +
                          SPCVAL((*WRKSTN))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             P1:        PMTCTL     CTL(MSGTYPE) COND((*EQ *INQ))
             DEP        CTL(*ALWAYS) PARM(&TOUSR &TOMSGQ) +
                          NBRTRUE((*EQ 1))
