/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* FNDSTRPDM: find a string in a file's members, then run an option */
/* on each member that has it or print what was found.              */
             CMD        PROMPT('Find String Using PDM') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHPDMCMD) HLPID(FNDSTRPDM)
             PARM       KWD(STRING) TYPE(*CHAR) LEN(40) MIN(1) +
                          CASE(*MIXED) PROMPT('Find string')
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(MBR) TYPE(*GENERIC) LEN(10) MIN(1) +
                          SPCVAL((*ALL)) MAX(300) PROMPT('Member')
             PARM       KWD(OPTION) TYPE(E1) PROMPT('Operation')
             PARM       KWD(PRTMBRLIST) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('Print list')
             PARM       KWD(PRTRCDS) TYPE(*CHAR) LEN(5) DFT(*NONE) +
                          SPCVAL((*NONE) (*ALL)) PROMPT('Print records')
             PARM       KWD(COL) TYPE(E2) PROMPT('Columns to search')
             PARM       KWD(CASE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*IGNORE) VALUES(*IGNORE *MATCH) +
                          PROMPT('Kind of match')
             PARM       KWD(PARM) TYPE(*CHAR) LEN(220) DFT(*NONE) +
                          SPCVAL((*NONE ' ')) CASE(*MIXED) +
                          PMTCTL(*PMTRQS) PROMPT('Parameters')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(5) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Option')
                        ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('Prompt')
             E2:        ELEM       TYPE(*INT2) DFT(1) RANGE(1 228) +
                          PROMPT('From column')
                        ELEM       TYPE(*CHAR) LEN(7) DFT(*RCDLEN) +
                          SPCVAL((*RCDLEN)) PROMPT('To column')
