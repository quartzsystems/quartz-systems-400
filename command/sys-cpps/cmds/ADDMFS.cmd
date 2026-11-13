/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDMFS: mount a file system over a directory (MOUNT is the same).  */
/* TYPE(*NFS) a remote export, TYPE(*UDFS) a user-defined file system. */
             CMD        PROMPT('Add Mounted FS') MAXPOS(3) +
                          ALLOW(*ALL) PMTOVRPGM(QSYS/QP0LCPOP) +
                          HLPPNLGRP(QHP0LCMD) HLPID(ADDMFS)
             PARM       KWD(TYPE) TYPE(*INT4) RSTD(*YES) MIN(1) +
                          SPCVAL((*NFS -1) (*UDFS -2)) KEYPARM(*YES) +
                          PROMPT('Type of file system')
             PARM       KWD(MFS) TYPE(*PNAME) LEN(5000) MIN(1) +
                          KEYPARM(*YES) CHOICE('Path name') +
                          PROMPT('File system to mount')
             PARM       KWD(MNTOVRDIR) TYPE(*PNAME) LEN(5000) MIN(1) +
                          KEYPARM(*YES) CHOICE('Path name') +
                          PROMPT('Directory to mount over')
             PARM       KWD(OPTIONS) TYPE(*CHAR) LEN(250) DFT(*DFT) +
                          CHOICE('Character value') +
                          PROMPT('Mount options')
             PARM       KWD(CCSID) TYPE(E1) PMTCTL(PNFS) +
                          PROMPT('Coded character set ID')
             PARM       KWD(CODEPAGE) TYPE(E2) PMTCTL(PCP) +
                          PROMPT('Code page')
 E1:         ELEM       TYPE(*INT4) DFT(*BINARY) RANGE(1 65533) +
                          SPCVAL((*ASCII -1) (*JOBCCSID -2) +
                          (*BINARY -3)) +
                          PROMPT('Data file CCSID')
             ELEM       TYPE(*INT4) DFT(*ASCII) RANGE(1 65533) +
                          SPCVAL((*ASCII -1) (*JOBCCSID -2)) +
                          PROMPT('Path name CCSID')
 E2:         ELEM       TYPE(*INT4) DFT(*BINARY) RANGE(1 32767) +
                          SPCVAL((*ASCII -1) (*JOBCCSID -2) +
                          (*BINARY -3)) +
                          PROMPT('Data file code page')
             ELEM       TYPE(*INT4) DFT(*ASCII) RANGE(1 32767) +
                          SPCVAL((*ASCII -1) (*JOBCCSID -2)) +
                          PROMPT('Path name code page')
 PNFS:       PMTCTL     CTL(TYPE) COND((*EQ *NFS)) NBRTRUE(*GT 0)
 PCP:        PMTCTL     CTL(CCSID) COND((*EQ 65536)) NBRTRUE(*EQ 1)
             DEP        CTL(*ALWAYS) PARM(&CCSID &CODEPAGE) +
                          NBRTRUE(*LE 1) MSGID(CPDB416)
