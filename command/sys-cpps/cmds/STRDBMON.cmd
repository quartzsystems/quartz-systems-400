/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRDBMON: start the database monitor over a job or every job. */
             CMD        PROMPT('Start Database Monitor') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(STRDBMON)
             PARM       KWD(OUTFILE) TYPE(Q1) MIN(1) +
                          PROMPT('File to receive output')
             PARM       KWD(OUTMBR) TYPE(E1) PROMPT('Output member options')
             PARM       KWD(JOB) TYPE(QJ) DFT(*) SNGVAL((*)) +
                          PROMPT('Job name')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*BASIC) VALUES(*BASIC *DETAIL *SUMMARY) +
                          PROMPT('Type of records')
             PARM       KWD(FRCRCD) TYPE(*INT2) DFT(*CALC) +
                          RANGE(0 32767) SPCVAL((*CALC 0)) +
                          PROMPT('Force record write')
             PARM       KWD(INLNBRRCD) TYPE(*INT4) DFT(*NONE) +
                          RANGE(0 2147483646) SPCVAL((*NONE -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Initial number of records')
             PARM       KWD(RUNTHLD) TYPE(*INT4) DFT(*NONE) +
                          RANGE(0 2147483647) SPCVAL((*NONE -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Run time threshold')
             PARM       KWD(STGTHLD) TYPE(*INT4) DFT(*NONE) +
                          RANGE(0 2147483647) SPCVAL((*NONE -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Storage threshold')
             PARM       KWD(INCSYSSQL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES *INI) PMTCTL(*PMTRQS) +
                          PROMPT('Include system SQL')
             PARM       KWD(FTRFILE) TYPE(Q2) DFT(*NONE) SNGVAL((*NONE)) +
                          MAX(10) PMTCTL(*PMTRQS) +
                          PROMPT('Filter by query file')
             PARM       KWD(FTRUSER) TYPE(*GENERIC) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE) (*CURRENT)) PMTCTL(*PMTRQS) +
                          PROMPT('Filter by user profile')
             PARM       KWD(FTRINTNETA) TYPE(*CHAR) LEN(254) DFT(*NONE) +
                          SPCVAL((*NONE) (*LOCAL)) PMTCTL(*PMTRQS) +
                          PROMPT('Filter by internet address')
             PARM       KWD(FTRLCLPORT) TYPE(*INT4) DFT(*NONE) +
                          RANGE(0 65535) SPCVAL((*NONE -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Filter by port number')
             PARM       KWD(FTRQRYGOVR) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL *COND) +
                          PMTCTL(*PMTRQS) PROMPT('Filter by query governor')
             PARM       KWD(FTRCLTACG) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Filter by accounting string')
             PARM       KWD(FTRCLTAPP) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Filter by application name')
             PARM       KWD(FTRCLTPGM) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Filter by program name')
             PARM       KWD(FTRCLTUSR) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Filter by client user ID')
             PARM       KWD(FTRCLTWS) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Filter by work station')
             PARM       KWD(COMMENT) TYPE(*CHAR) LEN(100) DFT(*BLANK) +
                          SPCVAL((*BLANK)) PROMPT('Monitor comment')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*GENERIC) LEN(10) MIN(1) +
                          SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Library')
             E1:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             QJ:        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL))
                        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL)) +
                          PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) SPCVAL((*ALL)) +
                          PROMPT('Number')
