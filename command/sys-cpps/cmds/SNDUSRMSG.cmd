/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SNDUSRMSG: send a message from a CL program and, for an inquiry, */
/* wait for the reply.                                                */
             CMD        PROMPT('Send User Message') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHMHCMD) HLPID(SNDUSRMSG)
             PARM       KWD(MSGID) TYPE(*NAME) LEN(7) +
                          PROMPT('Message identifier')
             PARM       KWD(MSGF) TYPE(Q1) PROMPT('Message file')
             PARM       KWD(MSGDTA) TYPE(*CHAR) LEN(3000) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Message data field values')
             PARM       KWD(MSG) TYPE(*CHAR) LEN(512) +
                          PROMPT('Message text')
             PARM       KWD(VALUES) TYPE(*CHAR) LEN(32) MAX(20) +
                          DFT(*NONE) SNGVAL((*NONE)) +
                          PROMPT('Valid replies')
             PARM       KWD(DFT) TYPE(*CHAR) LEN(32) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Default reply')
             PARM       KWD(MSGTYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*INQ) VALUES(*INQ *INFO) +
                          PROMPT('Message type')
             PARM       KWD(TOMSGQ) TYPE(Q2) PROMPT('To message queue')
             PARM       KWD(TOUSR) TYPE(*NAME) LEN(10) +
                          SPCVAL((*SYSOPR) (*REQUESTER)) +
                          PROMPT('To user profile')
             PARM       KWD(MSGRPY) TYPE(*CHAR) LEN(132) RTNVAL(*YES) +
                          PROMPT('CL var for MSGRPY')
             PARM       KWD(TRNTBL) TYPE(Q3) DFT(QSYSTRNTBL) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Translate table')
             Q1:        QUAL       TYPE(*NAME) LEN(10) DFT(QCPFMSG)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*EXT))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             DEP        CTL(*ALWAYS) PARM(&MSGID &MSG) NBRTRUE((*EQ 1))
