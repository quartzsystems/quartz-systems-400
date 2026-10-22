/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTJRNRCV: create a journal receiver. */
             CMD        PROMPT('Create Journal Receiver') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHJRCMD) HLPID(CRTJRNRCV)
             PARM       KWD(JRNRCV) TYPE(Q1) MIN(1) +
                          PROMPT('Journal receiver')
             PARM       KWD(ASP) TYPE(*DEC) LEN(3) DFT(*LIBASP) +
                          RANGE(1 32) SPCVAL((*LIBASP 0)) +
                          PROMPT('Journal receiver ASP')
             PARM       KWD(THRESHOLD) TYPE(*DEC) LEN(10) DFT(1500000) +
                          RANGE(1 1000000000) PMTCTL(*PMTRQS) +
                          PROMPT('Receiver size threshold (KB)')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) +
                          PROMPT('Public authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
