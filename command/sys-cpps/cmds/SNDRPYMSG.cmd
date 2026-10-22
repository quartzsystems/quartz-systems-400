/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SNDRPYMSG: answer an inquiry message by its key. */
             CMD        PROMPT('Send Reply Message') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(SNDRPYMSG)
             PARM       KWD(MSGKEY) TYPE(*CHAR) LEN(4) MIN(1) +
                          PROMPT('Message key')
             PARM       KWD(MSGQ) TYPE(Q1) MIN(1) PROMPT('Message queue')
             PARM       KWD(RPY) TYPE(*CHAR) LEN(132) DFT(*DFT) +
                          SPCVAL((*DFT)) PROMPT('Reply')
             PARM       KWD(RMV) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Remove message')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*JOB) RANGE(1 65535) +
                          SPCVAL((*JOB 0) (*HEX 65535)) +
                          PMTCTL(*PMTRQS) PROMPT('Coded character set ID')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
