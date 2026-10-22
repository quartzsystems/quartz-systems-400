/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRDBG: put the job in debug mode with programs and service */
/* programs under debug.                                          */
             CMD        PROMPT('Start Debug') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHDBGCMD) HLPID(STRDBG)
             PARM       KWD(PGM) TYPE(Q1) MAX(20) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Program')
             PARM       KWD(DFTPGM) TYPE(Q2) DFT(*PGM) SNGVAL((*PGM) +
                          (*NONE)) PROMPT('Default program')
             PARM       KWD(MAXTRC) TYPE(*INT4) DFT(200) RANGE(0 99999) +
                          PROMPT('Max trace statements')
             PARM       KWD(TRCFULL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*STOPTRC) VALUES(*STOPTRC *WRAP) +
                          PROMPT('Trace full')
             PARM       KWD(UPDPROD) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Update production files')
             PARM       KWD(OPMSRC) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('OPM source level debug')
             PARM       KWD(SRVPGM) TYPE(Q1) MAX(20) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Service program')
             PARM       KWD(CLS) TYPE(*PNAME) LEN(5000) MAX(20) +
                          DFT(*NONE) SNGVAL((*NONE)) PROMPT('Class file')
             PARM       KWD(DSPMODSRC) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*PGMDEP) VALUES(*PGMDEP *YES *NO) +
                          PROMPT('Display module source')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
