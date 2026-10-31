/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* INSPTF: load and apply every PTF of a package in requisite order, */
/* now or at the next IPL.                                           */
             CMD        PROMPT('Install PTF') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(INSPTF)
             PARM       KWD(LICPGM) TYPE(*CHAR) LEN(7) DFT(*ALL) +
                          MAX(300) SNGVAL((*ALL)) PROMPT('Product')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*SERVICE) (*SAVF)) PROMPT('Device')
             PARM       KWD(INSTYPE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*DLYALL) VALUES(*DLYALL *DLYIPL *IMMDLY +
                          *IMMONLY) PROMPT('Install type')
             PARM       KWD(PTFTYPE) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*ALLPTF) VALUES(*ALLPTF *HIPERLIC) +
                          PROMPT('PTF type')
             PARM       KWD(OMIT) TYPE(*CHAR) LEN(7) MAX(300) +
                          PROMPT('PTF numbers to omit')
             PARM       KWD(PTFGRP) TYPE(*NAME) LEN(60) DFT(*ALL) +
                          SPCVAL((*ALL)) PMTCTL(*PMTRQS) +
                          PROMPT('PTF group')
