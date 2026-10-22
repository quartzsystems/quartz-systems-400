/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTDTAQ: create a data queue, standard or DDM. */
             CMD        PROMPT('Create Data Queue') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(CRTDTAQ)
             PARM       KWD(DTAQ) TYPE(Q1) MIN(1) PROMPT('Data queue')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*STD) VALUES(*STD *DDM) PROMPT('Type')
             PARM       KWD(MAXLEN) TYPE(*INT4) RANGE(1 64512) +
                          PMTCTL(P1) PROMPT('Maximum entry length')
             PARM       KWD(FORCE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(P1) +
                          PROMPT('Force to auxiliary storage')
             PARM       KWD(SEQ) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*FIFO) VALUES(*FIFO *LIFO *KEYED) +
                          PMTCTL(P1) PROMPT('Sequence')
             PARM       KWD(KEYLEN) TYPE(*INT2) RANGE(1 256) +
                          PMTCTL(P3) PROMPT('Key length')
             PARM       KWD(SENDERID) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(P1) +
                          PROMPT('Include sender ID')
             PARM       KWD(SIZE) TYPE(E1) PMTCTL(P1) +
                          PROMPT('Queue size')
             PARM       KWD(AUTORCL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(P1) +
                          PROMPT('Automatic reclaim')
             PARM       KWD(RMTDTAQ) TYPE(Q2) PMTCTL(P2) +
                          PROMPT('Remote data queue')
             PARM       KWD(RMTLOCNAME) TYPE(*CHAR) LEN(255) +
                          PMTCTL(P2) PROMPT('Remote location')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) DFT(*LOC) +
                          SPCVAL((*LOC)) PMTCTL(P2) PROMPT('APPC device')
             PARM       KWD(LCLLOCNAME) TYPE(*NAME) LEN(8) DFT(*LOC) +
                          SPCVAL((*LOC) (*NETATR)) PMTCTL(P2) +
                          PROMPT('Local location')
             PARM       KWD(MODE) TYPE(*NAME) LEN(8) DFT(*NETATR) +
                          SPCVAL((*NETATR)) PMTCTL(P2) PROMPT('Mode')
             PARM       KWD(RMTNETID) TYPE(*NAME) LEN(8) DFT(*LOC) +
                          SPCVAL((*LOC) (*NETATR) (*NONE)) PMTCTL(P2) +
                          PROMPT('Remote network identifier')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) +
                          PROMPT('Authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT4) DFT(*MAX16MB) +
                          RANGE(1 16777216) +
                          SPCVAL((*MAX16MB 0) (*MAX2GB -1)) +
                          PROMPT('Maximum number of entries')
                        ELEM       TYPE(*INT4) DFT(16) +
                          RANGE(1 16777216) +
                          PROMPT('Initial number of entries')
             P1:        PMTCTL     CTL(TYPE) COND((*EQ *STD))
             P2:        PMTCTL     CTL(TYPE) COND((*EQ *DDM))
             P3:        PMTCTL     CTL(SEQ) COND((*EQ *KEYED))
