/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* EXPORTFS: export directories to NFS clients, or end their export, as */
/* the flags of OPTIONS say (CHGNFSEXP is the same command).          */
             CMD        PROMPT('Change NFS Export') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(EXPORTFS)
             PARM       KWD(OPTIONS) TYPE(*CHAR) LEN(200) DFT(*DFT) +
                          CHOICE('Character value') +
                          PROMPT('NFS export options')
             PARM       KWD(DIR) TYPE(*PNAME) LEN(5000) +
                          CHOICE('Path name') PROMPT('Directory')
             PARM       KWD(HOSTOPT) TYPE(E1) MAX(10) DFT(*DFT) +
                          SNGVAL((*DFT)) PMTCTL(*PMTRQS) +
                          PROMPT('Host options')
 E1:         ELEM       TYPE(*CHAR) LEN(256) MIN(1) +
                          CHOICE('Character value') PROMPT('Host name')
             ELEM       TYPE(*INT4) DFT(*BINARY) RANGE(0 65535) +
                          SPCVAL((*BINARY -1) (*ASCII -2) +
                          (*JOBCCSID -3)) +
                          PROMPT('Data file CCSID')
             ELEM       TYPE(*INT4) DFT(*ASCII) RANGE(0 65535) +
                          SPCVAL((*ASCII -1) (*JOBCCSID -2)) +
                          PROMPT('Path name CCSID')
             ELEM       TYPE(*INT4) RSTD(*YES) DFT(*SYNC) +
                          SPCVAL((*SYNC -1) (*ASYNC -2)) +
                          PROMPT('Force synchronous write')
             DEP        CTL(*ALWAYS) PARM((&OPTIONS *NE ' ') +
                          (&DIR *NE ' ')) NBRTRUE(*NE 0) MSGID(CPFA1C5)
