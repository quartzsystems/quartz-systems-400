/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RCVMSG: receive a message into CL variables, from a call stack */
/* entry's queue or a named message queue.                          */
             CMD        PROMPT('Receive Message') MAXPOS(*NONE) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHMHCMD) HLPID(RCVMSG)
             PARM       KWD(PGMQ) TYPE(E1) +
                          PROMPT('Call stack entry message queue')
             PARM       KWD(MSGQ) TYPE(Q1) PROMPT('Message queue')
             PARM       KWD(MSGTYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ANY) VALUES(*ANY *NEXT *PRV *INFO *INQ +
                          *RPY *COMP *DIAG *NOTIFY *COPY *EXCP *FIRST +
                          *LAST *RQS) PROMPT('Message type')
             PARM       KWD(MSGKEY) TYPE(*CHAR) LEN(8) DFT(*NONE) +
                          SPCVAL((*NONE) (*TOP)) PROMPT('Message key')
             PARM       KWD(WAIT) TYPE(*DEC) LEN(7 0) DFT(0) +
                          RANGE(0 9999999) SPCVAL((*MAX -1)) +
                          PROMPT('Wait time (seconds)')
             PARM       KWD(RMV) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO *KEEPEXCP) +
                          PROMPT('Remove message')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*JOB) RANGE(1 65535) +
                          SPCVAL((*JOB 0) (*HEX 65535)) +
                          PROMPT('Coded character set ID')
             PARM       KWD(KEYVAR) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('CL var for KEYVAR (4)')
             PARM       KWD(MSG) TYPE(*CHAR) LEN(3000) RTNVAL(*YES) +
                          PROMPT('CL var for MSG')
             PARM       KWD(MSGLEN) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for MSGLEN (5 0)')
             PARM       KWD(SECLVL) TYPE(*CHAR) LEN(3000) RTNVAL(*YES) +
                          PROMPT('CL var for SECLVL')
             PARM       KWD(SECLVLLEN) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for SECLVLLEN (5 0)')
             PARM       KWD(MSGDTA) TYPE(*CHAR) LEN(3000) RTNVAL(*YES) +
                          PROMPT('CL var for MSGDTA')
             PARM       KWD(MSGDTALEN) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for MSGDTALEN (5 0)')
             PARM       KWD(MSGID) TYPE(*CHAR) LEN(7) RTNVAL(*YES) +
                          PROMPT('CL var for MSGID (7)')
             PARM       KWD(SEV) TYPE(*DEC) LEN(2 0) RTNVAL(*YES) +
                          PROMPT('CL var for SEV (2 0)')
             PARM       KWD(SENDER) TYPE(*CHAR) LEN(720) RTNVAL(*YES) +
                          PROMPT('CL var for SENDER')
             PARM       KWD(RTNTYPE) TYPE(*CHAR) LEN(2) RTNVAL(*YES) +
                          PROMPT('CL var for RTNTYPE (2)')
             PARM       KWD(ALROPT) TYPE(*CHAR) LEN(9) RTNVAL(*YES) +
                          PROMPT('CL var for ALROPT (9)')
             PARM       KWD(MSGF) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for MSGF (10)')
             PARM       KWD(MSGFLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for MSGFLIB (10)')
             PARM       KWD(TXTCCSID) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for TXTCCSID (5 0)')
             PARM       KWD(DTACCSID) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for DTACCSID (5 0)')
             Q1:        QUAL       TYPE(*NAME) LEN(10) DFT(*PGMQ) +
                          SPCVAL((*PGMQ) (*EXT))
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
