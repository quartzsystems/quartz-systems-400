/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGDDMF: change a DDM file. */
             CMD        PROMPT('Change DDM File') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHDDCMD) HLPID(CHGDDMF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('DDM file')
             PARM       KWD(RMTFILE) TYPE(E1) DFT(*SAME) SNGVAL((*SAME)) +
                          PMTCTL(*PMTRQS) PROMPT('Remote file')
             PARM       KWD(RMTLOCNAME) TYPE(E2) SNGVAL((*RDB)) +
                          PMTCTL(*PMTRQS) PROMPT('Remote location')
             PARM       KWD(LVLCHK) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME S) (*RMTFILE Y) +
                          (*NO N)) PMTCTL(*PMTRQS) +
                          PROMPT('Record format level check')
             PARM       KWD(RDB) TYPE(*CHAR) LEN(18) DFT(*SAME) +
                          SPCVAL((*SAME)) PMTCTL(*PMTRQS) +
                          PROMPT('Relational database')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Text')
             PARM       KWD(DEV) TYPE(E3) PMTCTL(*PMTRQS) PROMPT('Device')
             PARM       KWD(LCLLOCNAME) TYPE(*CNAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME) (*LOC) (*NETATR)) +
                          PMTCTL(*PMTRQS) PROMPT('Local location')
             PARM       KWD(MODE) TYPE(*CNAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME) (*NETATR)) PMTCTL(*PMTRQS) +
                          PROMPT('Mode')
             PARM       KWD(RMTNETID) TYPE(*CNAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME) (*LOC) (*NETATR) (*NONE)) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Remote network identifier')
             PARM       KWD(PORT) TYPE(*INT4) DFT(*SAME) RANGE(1 65535) +
                          SPCVAL((*SAME -2) (*DRDA 446)) PMTCTL(*PMTRQS) +
                          PROMPT('Port number')
             PARM       KWD(ACCMTH) TYPE(E4) DFT(*SAME) +
                          SNGVAL((*SAME) (*RMTFILE) (*COMBINED)) +
                          PMTCTL(*PMTRQS) PROMPT('Access method')
             PARM       KWD(SHARE) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME S) (*NO N) (*YES Y)) +
                          PMTCTL(*PMTRQS) PROMPT('Share open data path')
             PARM       KWD(PTCCNV) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME S) (*NO N) (*YES Y)) +
                          PMTCTL(*PMTRQS) PROMPT('Protected conversation')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(Q2) DFT(*NONSTD) SNGVAL((*NONSTD)) +
                          PROMPT('File')
                        ELEM       TYPE(*CHAR) LEN(255) +
                          PROMPT('Nonstandard file name')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E2:        ELEM       TYPE(*CHAR) LEN(254) DFT(*SAME) +
                          SPCVAL((*SAME) (*DEVD)) PROMPT('Name or address')
                        ELEM       TYPE(*CHAR) LEN(10) RSTD(*YES) DFT(*SAME) +
                          VALUES(*SAME *SNA *IP) PROMPT('Type')
             E3:        ELEM       TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*LOC)) +
                          PROMPT('APPC device description')
             E4:        ELEM       TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*KEYED) SPCVAL((*KEYED K) (*ARRIVAL A)) +
                          PROMPT('Remote file attribute')
                        ELEM       TYPE(*CHAR) LEN(1) RSTD(*YES) DFT(*BOTH) +
                          SPCVAL((*BOTH B) (*RANDOM R) (*SEQUENTIAL S)) +
                          PROMPT('Local access method')
