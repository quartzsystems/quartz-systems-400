/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDRTGE: add a routing entry to a subsystem description. */
             CMD        PROMPT('Add Routing Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(ADDRTGE)
             PARM       KWD(SBSD) TYPE(Q1) MIN(1) +
                          PROMPT('Subsystem description')
             PARM       KWD(SEQNBR) TYPE(*INT2) MIN(1) RANGE(1 9999) +
                          PROMPT('Routing entry sequence number')
             PARM       KWD(CMPVAL) TYPE(E1) MIN(1) SNGVAL((*ANY)) +
                          PROMPT('Comparison data')
             PARM       KWD(PGM) TYPE(Q2) MIN(1) PROMPT('Program to call')
             PARM       KWD(CLS) TYPE(Q2) DFT(*SBSD) SNGVAL((*SBSD)) +
                          PROMPT('Class')
             PARM       KWD(MAXACT) TYPE(*INT4) DFT(*NOMAX) +
                          RANGE(1 1000) SPCVAL((*NOMAX -1)) +
                          PROMPT('Maximum active routing steps')
             PARM       KWD(POOLID) TYPE(*INT2) DFT(1) RANGE(1 10) +
                          PROMPT('Storage pool identifier')
             PARM       KWD(THDRSCAFN) TYPE(E2) DFT(*SYSVAL) +
                          SNGVAL((*SYSVAL)) PROMPT('Thread resources affinity')
             PARM       KWD(RSCAFNGRP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Resources affinity group')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(80) +
                          PROMPT('Compare value')
                        ELEM       TYPE(*INT2) DFT(1) RANGE(1 80) +
                          PROMPT('Starting position')
             E2:        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NOGROUP) VALUES(*NOGROUP *GROUP) +
                          PROMPT('Group')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NORMAL) VALUES(*NORMAL *HIGH) +
                          PROMPT('Level')
