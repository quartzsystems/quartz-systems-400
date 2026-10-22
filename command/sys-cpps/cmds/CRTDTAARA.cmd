/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTDTAARA: create a data area, local or DDM. */
             CMD        PROMPT('Create Data Area') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(CRTDTAARA)
             PARM       KWD(DTAARA) TYPE(Q1) MIN(1) PROMPT('Data area')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) MIN(1) +
                          VALUES(*DEC *CHAR *LGL *DDM) +
                          PROMPT('Type')
             PARM       KWD(LEN) TYPE(E1) PMTCTL(P1) PROMPT('Length')
             PARM       KWD(VALUE) TYPE(*X) LEN(2000) EXPR(*YES) +
                          PMTCTL(P1) PROMPT('Initial value')
             PARM       KWD(RMTDTAARA) TYPE(Q2) PMTCTL(P2) +
                          PROMPT('Remote data area')
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
             E1:        ELEM       TYPE(*INT2) RANGE(1 2000) +
                          PROMPT('Length')
                        ELEM       TYPE(*INT2) DFT(0) RANGE(0 9) +
                          PROMPT('Decimal positions')
             P1:        PMTCTL     CTL(TYPE) COND((*NE *DDM))
             P2:        PMTCTL     CTL(TYPE) COND((*EQ *DDM))
