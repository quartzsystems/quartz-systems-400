/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTCLPGM: compile CL source into an OPM program. */
             CMD        PROMPT('Create CL Program') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(CRTCLPGM)
             PARM       KWD(PGM) TYPE(Q1) MIN(1) PROMPT('Program')
             PARM       KWD(SRCFILE) TYPE(Q2) PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*PGM) +
                          SPCVAL((*PGM)) PROMPT('Source member')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SRCMBRTXT) +
                          SPCVAL((*SRCMBRTXT) (*BLANK ' ')) +
                          PROMPT('Description')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*GEN *NOSRC *NOXREF *NOSECLVL) MAX(4) +
                          VALUES(*GEN *NOGEN *SRC *SOURCE *NOSRC +
                          *NOSOURCE *XREF *NOXREF *SECLVL *NOSECLVL) +
                          PMTCTL(*PMTRQS) PROMPT('Source listing options')
             PARM       KWD(GENOPT) TYPE(*CHAR) LEN(11) RSTD(*YES) +
                          DFT(*NOLIST *NOXREF *NOPATCH *NODUMP *NOATR +
                          *NOOPTIMIZE) MAX(6) VALUES(*NOLIST *LIST +
                          *NOXREF *XREF *NOPATCH *PATCH *NODUMP *DUMP +
                          *NOATR *ATR *NOOPTIMIZE *OPTIMIZE) +
                          PMTCTL(*PMTRQS) PROMPT('Generation options')
             PARM       KWD(USRPRF) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*USER) VALUES(*USER *OWNER) +
                          PMTCTL(*PMTRQS) PROMPT('User profile')
             PARM       KWD(LOG) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*JOB) +
                          VALUES(*JOB *YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Log commands')
             PARM       KWD(ALWRTVSRC) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Allow RTVCLSRC')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Replace program')
             PARM       KWD(TGTRLS) TYPE(*CHAR) LEN(8) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*PRV)) PMTCTL(*PMTRQS) +
                          PROMPT('Target release')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             PARM       KWD(SRTSEQ) TYPE(Q3) DFT(*HEX) SNGVAL((*HEX) +
                          (*JOB) (*JOBRUN) (*LANGIDUNQ) (*LANGIDSHR)) +
                          PMTCTL(*PMTRQS) PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*NAME) LEN(3) DFT(*JOBRUN) +
                          SPCVAL((*JOBRUN) (*JOB)) PMTCTL(*PMTRQS) +
                          PROMPT('Language ID')
             PARM       KWD(INCFILE) TYPE(Q4) DFT(*SRCFILE) +
                          SNGVAL((*SRCFILE)) PMTCTL(*PMTRQS) +
                          PROMPT('Include file')
             PARM       KWD(DBGENCKEY) TYPE(*CHAR) LEN(16) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) PMTCTL(*PMTRQS) +
                          PROMPT('Debug encryption key')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) DFT(QCLSRC)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q4:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
