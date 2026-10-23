/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTMSGF: create a message file. */
             CMD        PROMPT('Create Message File') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(CRTMSGF)
             PARM       KWD(MSGF) TYPE(Q1) MIN(1) PROMPT('Message file')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(SIZE) TYPE(E1) PMTCTL(*PMTRQS) +
                          PROMPT('Message file size')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*HEX) +
                          RANGE(1 65535) +
                          SPCVAL((*HEX 65535) (*JOB 0) (*MSGD 65534)) +
                          PMTCTL(*PMTRQS) PROMPT('Coded character set ID')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) +
                          PROMPT('Authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT4) DFT(10) RANGE(0 16384) +
                          PROMPT('Initial storage size (KB)')
                        ELEM       TYPE(*INT4) DFT(2) RANGE(0 16384) +
                          PROMPT('Increment (KB)')
                        ELEM       TYPE(*INT4) DFT(*NOMAX) RANGE(0 9999) +
                          SPCVAL((*NOMAX -1)) +
                          PROMPT('Maximum increments')
