/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDRPYLE: add an entry to the system reply list. */
             CMD        PROMPT('Add Reply List Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(ADDRPYLE)
             PARM       KWD(SEQNBR) TYPE(*DEC) LEN(4 0) MIN(1) +
                          RANGE(1 9999) PROMPT('Sequence number')
             PARM       KWD(MSGID) TYPE(*GENERIC) LEN(7) MIN(1) +
                          PROMPT('Message identifier')
             PARM       KWD(CMPDTA) TYPE(E1) DFT(*NONE) SNGVAL((*NONE)) +
                          PROMPT('Comparison data')
             PARM       KWD(RPY) TYPE(*CHAR) LEN(32) DFT(*DFT) +
                          SPCVAL((*DFT) (*RQD)) PROMPT('Message reply')
             PARM       KWD(DMPJOB) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('Dump job')
             E1:        ELEM       TYPE(*CHAR) LEN(28) MIN(1) +
                          PROMPT('Comparison data')
                        ELEM       TYPE(*INT2) DFT(1) RANGE(1 999) +
                          PROMPT('Start position')
