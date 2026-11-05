/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRDBMON: start the database monitor over a job or every job. */
             CMD        PROMPT('Start Database Monitor') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(STRDBMON)
             PARM       KWD(OUTFILE) TYPE(Q1) MIN(1) +
                          PROMPT('File to receive output')
             PARM       KWD(OUTMBR) TYPE(E1) PROMPT('Output member options')
             PARM       KWD(JOB) TYPE(EJ) PROMPT('Job name')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*DETAIL) VALUES(*DETAIL *SUMMARY *BASIC) +
                          PROMPT('Type of records')
             PARM       KWD(FRCRCD) TYPE(*INT2) DFT(*CALC) +
                          RANGE(0 32767) SPCVAL((*CALC 0)) +
                          PROMPT('Force record write')
             PARM       KWD(INLNBRRCD) TYPE(*INT4) DFT(*NONE) +
                          RANGE(0 2147483646) SPCVAL((*NONE -1)) +
                          PROMPT('Initial number of records')
             PARM       KWD(RUNTHLD) TYPE(*INT4) DFT(*NONE) +
                          RANGE(0 2147483647) SPCVAL((*NONE -1)) +
                          PROMPT('Run time threshold')
             PARM       KWD(STGTHLD) TYPE(*INT4) DFT(*NONE) +
                          RANGE(0 2147483647) SPCVAL((*NONE -1)) +
                          PROMPT('Storage threshold')
             PARM       KWD(INCSYSSQL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Include system SQL')
             PARM       KWD(FTRFILE) TYPE(EF) DFT(*NONE) SNGVAL((*NONE)) +
                          MAX(10) PROMPT('Filter by database file')
             PARM       KWD(FTRUSER) TYPE(EU) MAX(10) +
                          PROMPT('Filter by user profile')
             PARM       KWD(FTRINTNETA) TYPE(*CHAR) LEN(254) DFT(*NONE) +
                          SPCVAL((*NONE) (*LOCAL)) +
                          PROMPT('Filter by internet address')
             PARM       KWD(FTRLCLPORT) TYPE(*INT4) DFT(*NONE) +
                          RANGE(0 65535) SPCVAL((*NONE -1)) +
                          PROMPT('Filter by local port number')
             PARM       KWD(FTRQRYGOVR) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL *COND) +
                          PROMPT('Filter by query governor')
             PARM       KWD(FTRCLTACG) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Filter by accounting string')
             PARM       KWD(FTRCLTAPP) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Filter by application name')
             PARM       KWD(FTRCLTPGM) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Filter by program name')
             PARM       KWD(FTRCLTUSR) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Filter by client user ID')
             PARM       KWD(FTRCLTWS) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Filter by work station')
             PARM       KWD(FTRSQLCODE) TYPE(*INT4) DFT(*NONE) +
                          RANGE(-99999 99999) SPCVAL((*NONE -1000000) +
                          (*NONZERO -1000001) (*ERROR -1000002) +
                          (*WARN -1000003)) PROMPT('Filter by SQLCODE')
             PARM       KWD(HOSTVAR) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*BASIC) VALUES(*BASIC *CONDENSED *SECURE) +
                          PROMPT('Host Variable Values')
             PARM       KWD(COMMENT) TYPE(*CHAR) LEN(100) DFT(*BLANK) +
                          SPCVAL((*BLANK)) PROMPT('Comment')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('Library')
             EF:        ELEM       TYPE(Q2) PROMPT('File')
                        ELEM       TYPE(*CHAR) LEN(3) RSTD(*YES) +
                          VALUES(*EQ *NE) PROMPT('Filter operator')
             EU:        ELEM       TYPE(*GENERIC) LEN(10) +
                          SPCVAL((*CURRENT)) PROMPT('User')
                        ELEM       TYPE(*CHAR) LEN(3) RSTD(*YES) +
                          DFT(*EQ) VALUES(*EQ *NE) PROMPT('Filter operator')
             EJ:        ELEM       TYPE(QJ) PROMPT('Name')
                        ELEM       TYPE(*CHAR) LEN(3) RSTD(*YES) +
                          DFT(*EQ) VALUES(*EQ *NE) PROMPT('Filter operator')
             E1:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             QJ:        QUAL       TYPE(*GENERIC) LEN(10) DFT(*) +
                          SPCVAL((*) (*ALL))
                        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL)) +
                          PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) SPCVAL((*ALL)) +
                          PROMPT('Number')
