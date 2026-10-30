/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* VRYCFG: vary configuration objects on or off. */
             CMD        PROMPT('Vary Configuration') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHDCCMD1) HLPID(VRYCFG)
             PARM       KWD(CFGOBJ) TYPE(*GENERIC) LEN(10) MIN(1) +
                          MAX(256) SNGVAL((*APPN) (*PRVCFGTYPE)) +
                          PROMPT('Configuration object')
             PARM       KWD(CFGTYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          MIN(1) VALUES(*NWS *LIN *CTL *DEV *MLBRSC) +
                          PROMPT('Type')
             PARM       KWD(STATUS) TYPE(*CHAR) LEN(12) RSTD(*YES) +
                          MIN(1) VALUES(*ON *OFF *RESET *ALLOCATE +
                          *UNPROTECTED *DEALLOCATE) PROMPT('Status')
             PARM       KWD(RANGE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NET) VALUES(*NET *OBJ) PMTCTL(PLCD) +
                          PROMPT('Range')
             PARM       KWD(VRYWAIT) TYPE(*INT2) DFT(*CFGOBJ) +
                          RANGE(15 180) SPCVAL((*CFGOBJ -1) (*NOWAIT 0)) +
                          PMTCTL(PLCD) PROMPT('Vary on wait')
             PARM       KWD(ASCVRYOFF) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(PLCD) +
                          PROMPT('Asynchronous vary off')
             PARM       KWD(RESET) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(PLCD) +
                          PROMPT('Reset')
             PARM       KWD(RSRCNAME) TYPE(*NAME) LEN(10) MAX(16) +
                          SNGVAL((*ALL)) PMTCTL(PMLB) +
                          PROMPT('Resource name')
             PARM       KWD(FRCVRYOFF) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES *LOCK) PMTCTL(PLCD) +
                          PROMPT('Forced vary off')
             PARM       KWD(SBMMLTJOB) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(PLCD) +
                          PROMPT('Submit multiple jobs')
             PARM       KWD(JOBD) TYPE(Q1) PMTCTL(PLCD) +
                          PROMPT('Job description')
             PARM       KWD(GENPTHCERT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(PNWS) +
                          PROMPT('Generate path certificate')
             PARM       KWD(RESETSYS) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(PNWS) +
                          PROMPT('Reset system')
 Q1:         QUAL       TYPE(*NAME) LEN(10) DFT(QBATCH)
             QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) SPCVAL((*LIBL)) +
                          PROMPT('Library')
 PLCD:       PMTCTL     CTL(CFGTYPE) COND((*NE *MLBRSC))
 PMLB:       PMTCTL     CTL(CFGTYPE) COND((*EQ *MLBRSC))
 PNWS:       PMTCTL     CTL(CFGTYPE) COND((*EQ *NWS))
