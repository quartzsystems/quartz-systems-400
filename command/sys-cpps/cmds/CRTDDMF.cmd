/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTDDMF: a DDM file, a local name for a file on another system. */
             CMD        PROMPT('Create DDM File') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHDDCMD) HLPID(CRTDDMF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('DDM file')
             PARM       KWD(RMTFILE) TYPE(E1) MIN(1) PROMPT('Remote file')
             PARM       KWD(RMTLOCNAME) TYPE(E2) MIN(1) SNGVAL((*RDB)) +
                          PROMPT('Remote location')
             PARM       KWD(RDB) TYPE(*CHAR) LEN(18) PMTCTL(*PMTRQS) +
                          PROMPT('Relational database')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Text')
             PARM       KWD(DEV) TYPE(E3) PMTCTL(*PMTRQS) PROMPT('Device')
             PARM       KWD(LCLLOCNAME) TYPE(*CNAME) LEN(8) DFT(*LOC) +
                          SPCVAL((*LOC) (*NETATR)) PMTCTL(*PMTRQS) +
                          PROMPT('Local location')
             PARM       KWD(MODE) TYPE(*CNAME) LEN(8) DFT(*NETATR) +
                          SPCVAL((*NETATR)) PMTCTL(*PMTRQS) PROMPT('Mode')
             PARM       KWD(RMTNETID) TYPE(*CNAME) LEN(8) DFT(*LOC) +
                          SPCVAL((*LOC) (*NETATR) (*NONE)) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Remote network identifier')
             PARM       KWD(PORT) TYPE(*INT4) DFT(*DRDA) RANGE(1 65535) +
                          SPCVAL((*DRDA 446)) PMTCTL(*PMTRQS) +
                          PROMPT('Port number')
             PARM       KWD(ACCMTH) TYPE(E4) DFT(*RMTFILE) +
                          SNGVAL((*RMTFILE) (*COMBINED)) +
                          PMTCTL(*PMTRQS) PROMPT('Access method')
             PARM       KWD(SHARE) TYPE(*CHAR) LEN(1) RSTD(*YES) DFT(*NO) +
                          SPCVAL((*NO N) (*YES Y)) PMTCTL(*PMTRQS) +
                          PROMPT('Share open data path')
             PARM       KWD(PTCCNV) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*NO) SPCVAL((*NO N) (*YES Y)) +
                          PMTCTL(*PMTRQS) PROMPT('Protected conversation')
             PARM       KWD(LVLCHK) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*RMTFILE) SPCVAL((*RMTFILE Y) (*NO N)) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Record format level check')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*ALL) (*CHANGE) +
                          (*EXCLUDE) (*USE)) PMTCTL(*PMTRQS) +
                          PROMPT('Authority')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*YES) SPCVAL((*YES Y) (*NO N)) +
                          PMTCTL(*PMTRQS) PROMPT('Replace file')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(Q2) SNGVAL((*NONSTD)) PROMPT('File')
                        ELEM       TYPE(*CHAR) LEN(255) +
                          PROMPT('Nonstandard file name')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E2:        ELEM       TYPE(*CHAR) LEN(254) MIN(1) +
                          SPCVAL((*DEVD)) PROMPT('Name or address')
                        ELEM       TYPE(*CHAR) LEN(10) RSTD(*YES) DFT(*SNA) +
                          VALUES(*SNA *IP) PROMPT('Type')
             E3:        ELEM       TYPE(*NAME) LEN(10) DFT(*LOC) +
                          SPCVAL((*LOC)) PROMPT('APPC device description')
             E4:        ELEM       TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*KEYED) SPCVAL((*KEYED K) (*ARRIVAL A)) +
                          PROMPT('Remote file attribute')
                        ELEM       TYPE(*CHAR) LEN(1) RSTD(*YES) DFT(*BOTH) +
                          SPCVAL((*BOTH B) (*RANDOM R) (*SEQUENTIAL S)) +
                          PROMPT('Local access method')
