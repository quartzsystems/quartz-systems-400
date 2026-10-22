/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTMSGQ: create a message queue. */
             CMD        PROMPT('Create Message Queue') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(CRTMSGQ)
             PARM       KWD(MSGQ) TYPE(Q1) MIN(1) PROMPT('Message queue')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(FORCE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Force to auxiliary storage')
             PARM       KWD(SIZE) TYPE(E1) PMTCTL(*PMTRQS) +
                          PROMPT('Message queue size')
             PARM       KWD(ALWALT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Allow alerts')
             PARM       KWD(MSGQFULL) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SNDRCV) VALUES(*SNDRCV *NOWRAP *WRAP) +
                          PMTCTL(*PMTRQS) PROMPT('Queue full action')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) +
                          PROMPT('Authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT4) DFT(5) RANGE(1 16384) +
                          PROMPT('Initial storage size (KB)')
                        ELEM       TYPE(*INT4) DFT(1) RANGE(1 16384) +
                          PROMPT('Increment (KB)')
                        ELEM       TYPE(*INT4) DFT(*NOMAX) RANGE(0 9999) +
                          SPCVAL((*NOMAX -1)) +
                          PROMPT('Maximum increments')
