/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RTVMSG: put a message description from a message file, with its */
/* substitution data filled in, into CL variables.                   */
             CMD        PROMPT('Retrieve Message') MAXPOS(2) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(RTVMSG)
             PARM       KWD(MSGID) TYPE(*NAME) LEN(7) MIN(1) +
                          PROMPT('Message identifier')
             PARM       KWD(MSGF) TYPE(Q1) MIN(1) PROMPT('Message file')
             PARM       KWD(MSGDTA) TYPE(*CHAR) LEN(512) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Message data field values')
             PARM       KWD(MSG) TYPE(*CHAR) LEN(132) RTNVAL(*YES) +
                          PROMPT('CL var for MSG (132)')
             PARM       KWD(MSGLEN) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for MSGLEN (5 0)')
             PARM       KWD(SECLVL) TYPE(*CHAR) LEN(3000) RTNVAL(*YES) +
                          PROMPT('CL var for SECLVL (3000)')
             PARM       KWD(SECLVLLEN) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for SECLVLLEN (5 0)')
             PARM       KWD(SEV) TYPE(*DEC) LEN(2 0) RTNVAL(*YES) +
                          PROMPT('CL var for SEV (2 0)')
             PARM       KWD(ALROPT) TYPE(*CHAR) LEN(9) RTNVAL(*YES) +
                          PROMPT('CL var for ALROPT (9)')
             PARM       KWD(LOGPRB) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('CL var for LOGPRB (4)')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*JOB) RANGE(1 65535) +
                          SPCVAL((*JOB 0) (*HEX 65535)) PROMPT('CCSID')
             PARM       KWD(TXTCCSID) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for TXTCCSID (5 0)')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
