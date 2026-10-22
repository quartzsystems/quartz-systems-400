/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGPJE: change a prestart job entry of a subsystem description. */
             CMD        PROMPT('Change Prestart Job Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CHGPJE)
             PARM       KWD(SBSD) TYPE(Q1) MIN(1) +
                          PROMPT('Subsystem description')
             PARM       KWD(PGM) TYPE(Q1) MIN(1) PROMPT('Program')
             PARM       KWD(USER) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('User profile')
             PARM       KWD(STRJOBS) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Start jobs')
             PARM       KWD(INLJOBS) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 9999) SPCVAL((*SAME -1)) +
                          PROMPT('Initial number of jobs')
             PARM       KWD(THRESHOLD) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 9999) SPCVAL((*SAME -1)) +
                          PROMPT('Threshold')
             PARM       KWD(ADLJOBS) TYPE(*INT4) DFT(*SAME) +
                          RANGE(0 999) SPCVAL((*SAME -1)) +
                          PROMPT('Additional number of jobs')
             PARM       KWD(MAXJOBS) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 9999) SPCVAL((*SAME -2) (*NOMAX -1)) +
                          PROMPT('Maximum number of jobs')
             PARM       KWD(MAXUSE) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 1000) SPCVAL((*SAME -2) (*NOMAX -1)) +
                          PROMPT('Maximum number of uses')
             PARM       KWD(WAIT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Wait for job')
             PARM       KWD(POOLID) TYPE(*INT2) DFT(*SAME) RANGE(1 10) +
                          SPCVAL((*SAME -1)) PROMPT('Pool identifier')
             PARM       KWD(CLS) TYPE(E1) MAX(2) DFT(*SAME) +
                          SNGVAL((*SAME)) PROMPT('Class')
             PARM       KWD(JOBD) TYPE(Q2) DFT(*SAME) +
                          SNGVAL((*SAME) (*USRPRF)) PROMPT('Job description')
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
             E1:        ELEM       TYPE(Q2) DFT(*SBSD) SNGVAL((*SBSD)) +
                          PROMPT('Class')
                        ELEM       TYPE(*INT4) DFT(*CALC) RANGE(1 32767) +
                          SPCVAL((*CALC 0)) PROMPT('Number of jobs to use class')
             E2:        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NOGROUP) VALUES(*NOGROUP *GROUP) +
                          PROMPT('Group')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NORMAL) VALUES(*NORMAL *HIGH) +
                          PROMPT('Level')
