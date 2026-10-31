/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTSAVF: create a save file. */
             CMD        PROMPT('Create Save File') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHSRCMD) HLPID(CRTSAVF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('Save file')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK '')) PROMPT('Text ''description''')
             PARM       KWD(ASP) TYPE(*INT2) DFT(1) RANGE(1 255) +
                          PMTCTL(*PMTRQS) PROMPT('Auxiliary storage pool ID')
             PARM       KWD(MAXRCDS) TYPE(*INT4) DFT(*NOMAX) +
                          RANGE(1 2147483647) SPCVAL((*NOMAX 0)) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum records')
             PARM       KWD(WAITFILE) TYPE(*INT4) DFT(*IMMED) +
                          RANGE(1 32767) SPCVAL((*IMMED 0) (*CLS -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum file wait time')
             PARM       KWD(SHARE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Share open data path')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*EXCLUDE) +
                          SPCVAL((*EXCLUDE) (*ALL) (*CHANGE) (*USE) +
                          (*LIBCRTAUT)) PMTCTL(*PMTRQS) PROMPT('Authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
