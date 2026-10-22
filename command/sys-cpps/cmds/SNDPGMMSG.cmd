/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SNDPGMMSG: send a message from a CL program to a call stack entry, */
/* a message queue or a user.                                         */
             CMD        PROMPT('Send Program Message') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHMHCMD) HLPID(SNDPGMMSG)
             PARM       KWD(MSGID) TYPE(*NAME) LEN(7) +
                          PROMPT('Message identifier')
             PARM       KWD(MSGF) TYPE(Q1) PROMPT('Message file')
             PARM       KWD(MSGDTA) TYPE(*CHAR) LEN(3000) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Message data field values')
             PARM       KWD(MSG) TYPE(*CHAR) LEN(3000) +
                          PROMPT('Message text')
             PARM       KWD(TOPGMQ) TYPE(E1) DFT(*PRV) SNGVAL((*EXT)) +
                          PROMPT('To call stack entry queue')
             PARM       KWD(TOMSGQ) TYPE(Q2) MAX(50) DFT(*TOPGMQ) +
                          SNGVAL((*TOPGMQ) (*SYSOPR)) +
                          PROMPT('To message queue')
             PARM       KWD(TOUSR) TYPE(*NAME) LEN(10) +
                          SPCVAL((*REQUESTER) (*SYSOPR) (*ALLACT)) +
                          PROMPT('To user profile')
             PARM       KWD(MSGTYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*INFO) VALUES(*INFO *INQ *COMP *DIAG +
                          *NOTIFY *ESCAPE *RQS *STATUS) +
                          PROMPT('Message type')
             PARM       KWD(RPYMSGQ) TYPE(Q3) PMTCTL(P1) +
                          PROMPT('Message queue to get reply')
             PARM       KWD(KEYVAR) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('CL var for KEYVAR (4)')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*JOB) RANGE(1 65535) +
                          SPCVAL((*JOB 0) (*HEX 65535)) +
                          PMTCTL(*PMTRQS) PROMPT('Coded character set ID')
             Q1:        QUAL       TYPE(*NAME) LEN(10) DFT(QCPFMSG)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10) DFT(*PGMQ) +
                          SPCVAL((*PGMQ))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*PRV) VALUES(*PRV *SAME) +
                          PROMPT('Relationship')
                        ELEM       TYPE(Q4) PROMPT('Call stack entry')
             Q4:        QUAL       TYPE(*CHAR) LEN(10) DFT(*) +
                          SPCVAL((*) (*CTLBDY) (*PGMBDY) (*PGMNAME)) +
                          PROMPT('Call stack entry')
                        QUAL       TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Module')
                        QUAL       TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Bound program')
             P1:        PMTCTL     CTL(MSGTYPE) COND((*EQ *INQ) +
                          (*EQ *NOTIFY)) NBRTRUE((*EQ 1))
             DEP        CTL(*ALWAYS) PARM(&MSGID &MSG) NBRTRUE((*EQ 1))
