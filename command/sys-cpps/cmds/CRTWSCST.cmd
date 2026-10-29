/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTWSCST: compile a workstation customizing object from a source */
/* member (the transform's commands for one printer).                */
             CMD        PROMPT('Create WSCST') MAXPOS(3) ALLOW(*ALL) +
                          HLPPNLGRP(QHUCCMD) HLPID(CRTWSCST)
             PARM       KWD(WSCST) TYPE(Q1) MIN(1) +
                          PROMPT('Workstation customizing object')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*WSCST) +
                          SPCVAL((*WSCST)) PROMPT('Source member')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SRCMBRTXT) +
                          SPCVAL((*SRCMBRTXT) (*BLANK ' ')) +
                          PROMPT('Text ''description''')
             PARM       KWD(SRCFILE) TYPE(Q2) PMTCTL(*PMTRQS) +
                          PROMPT('Source file')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Replace object')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) DFT(QTXTSRC)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
