/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGBCKUP: change what a backup saves and where. */
             CMD        PROMPT('Change Backup Options') MAXPOS(1) +
                          ALLOW(*ALL) PMTOVRPGM(QSYS/QEZBCKPO) +
                          HLPPNLGRP(QHWMCMD) HLPID(CHGBCKUP)
             PARM       KWD(BCKUPOPT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          MIN(1) VALUES(*DAILY *WEEKLY *MONTHLY *ALL) +
                          KEYPARM(*YES) PROMPT('Backup options')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MAX(4) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) PROMPT('Device')
             PARM       KWD(TAPSET) TYPE(*NAME) LEN(4) MAX(7) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) PROMPT('Tape set')
             PARM       KWD(CLRTAP) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Clear the tape')
             PARM       KWD(SBMJOB) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Submit to batch')
             PARM       KWD(CHGONLY) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Changed objects only')
             PARM       KWD(PRTRPT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Print detailed report')
             PARM       KWD(LIB) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ALLUSR *BCKUPL *NONE) +
                          PROMPT('Library')
             PARM       KWD(FLR) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ALL *BCKUPL *NONE) +
                          PROMPT('Folder')
             PARM       KWD(DIR) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ALLUSR *NONE) +
                          PROMPT('Directory')
             PARM       KWD(SECDTA) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Save security data')
             PARM       KWD(CFG) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Save configuration data')
             PARM       KWD(MAIL) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Save mail')
             PARM       KWD(CAL) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Save calendar data')
             PARM       KWD(EXITPGM) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) PROMPT('User exit program')
 Q1:         QUAL       TYPE(*NAME) LEN(10)
             QUAL       TYPE(*NAME) LEN(10) PROMPT('Library')
