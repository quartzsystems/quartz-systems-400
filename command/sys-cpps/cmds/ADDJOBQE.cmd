/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDJOBQE: add a job queue entry to a subsystem description. */
             CMD        PROMPT('Add Job Queue Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(ADDJOBQE)
             PARM       KWD(SBSD) TYPE(Q1) MIN(1) +
                          PROMPT('Subsystem description')
             PARM       KWD(JOBQ) TYPE(Q1) MIN(1) PROMPT('Job queue')
             PARM       KWD(MAXACT) TYPE(*INT4) DFT(1) RANGE(1 1000) +
                          SPCVAL((*NOMAX -1)) PROMPT('Maximum active jobs')
             PARM       KWD(SEQNBR) TYPE(*INT2) DFT(10) RANGE(1 9999) +
                          PROMPT('Sequence number')
             PARM       KWD(MAXPTY1) TYPE(*INT4) DFT(*NOMAX) RANGE(0 99) +
                          SPCVAL((*NOMAX -1)) PROMPT('Max active priority 1')
             PARM       KWD(MAXPTY2) TYPE(*INT4) DFT(*NOMAX) RANGE(0 99) +
                          SPCVAL((*NOMAX -1)) PROMPT('Max active priority 2')
             PARM       KWD(MAXPTY3) TYPE(*INT4) DFT(*NOMAX) RANGE(0 99) +
                          SPCVAL((*NOMAX -1)) PROMPT('Max active priority 3')
             PARM       KWD(MAXPTY4) TYPE(*INT4) DFT(*NOMAX) RANGE(0 99) +
                          SPCVAL((*NOMAX -1)) PROMPT('Max active priority 4')
             PARM       KWD(MAXPTY5) TYPE(*INT4) DFT(*NOMAX) RANGE(0 99) +
                          SPCVAL((*NOMAX -1)) PROMPT('Max active priority 5')
             PARM       KWD(MAXPTY6) TYPE(*INT4) DFT(*NOMAX) RANGE(0 99) +
                          SPCVAL((*NOMAX -1)) PROMPT('Max active priority 6')
             PARM       KWD(MAXPTY7) TYPE(*INT4) DFT(*NOMAX) RANGE(0 99) +
                          SPCVAL((*NOMAX -1)) PROMPT('Max active priority 7')
             PARM       KWD(MAXPTY8) TYPE(*INT4) DFT(*NOMAX) RANGE(0 99) +
                          SPCVAL((*NOMAX -1)) PROMPT('Max active priority 8')
             PARM       KWD(MAXPTY9) TYPE(*INT4) DFT(*NOMAX) RANGE(0 99) +
                          SPCVAL((*NOMAX -1)) PROMPT('Max active priority 9')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
