/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGRTGE: change a routing entry of a subsystem description. */
             CMD        PROMPT('Change Routing Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CHGRTGE)
             PARM       KWD(SBSD) TYPE(Q1) MIN(1) +
                          PROMPT('Subsystem description')
             PARM       KWD(SEQNBR) TYPE(*INT2) MIN(1) RANGE(1 9999) +
                          PROMPT('Routing entry sequence number')
             PARM       KWD(CMPVAL) TYPE(E1) DFT(*SAME) +
                          SNGVAL((*SAME) (*ANY)) PROMPT('Comparison data')
             PARM       KWD(PGM) TYPE(Q2) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Program to call')
             PARM       KWD(CLS) TYPE(Q2) DFT(*SAME) +
                          SNGVAL((*SAME) (*SBSD)) PROMPT('Class')
             PARM       KWD(MAXACT) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 1000) SPCVAL((*SAME -2) (*NOMAX -1)) +
                          PROMPT('Maximum active routing steps')
             PARM       KWD(POOLID) TYPE(*INT2) DFT(*SAME) RANGE(1 10) +
                          SPCVAL((*SAME -1)) +
                          PROMPT('Storage pool identifier')
             PARM       KWD(THDRSCAFN) TYPE(E2) DFT(*SAME) +
                          SNGVAL((*SAME) (*SYSVAL)) +
                          PROMPT('Thread resources affinity')
             PARM       KWD(RSCAFNGRP) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
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
