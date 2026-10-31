/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPOPT: display an optical volume's attributes, directories,      */
/* files, or the saves on it.                                         */
             CMD        PROMPT('Display Optical') MAXPOS(3) ALLOW(*ALL) +
                          HLPPNLGRP(QHOPCMD) HLPID(DSPOPT)
             PARM       KWD(VOL) TYPE(*CHAR) LEN(32) MIN(1) +
                          SPCVAL((*MOUNTED)) PROMPT('Volume identifier')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Optical device')
             PARM       KWD(DATA) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*VOLATR) VALUES(*VOLATR *DIRATR *FILATR +
                          *SAVRST) PROMPT('Data type')
             PARM       KWD(PATH) TYPE(*PNAME) LEN(1024) DFT('/') +
                          CASE(*MIXED) PMTCTL(PD) PROMPT('Path')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             PD:        PMTCTL     CTL(DATA) COND((*NE *VOLATR))
