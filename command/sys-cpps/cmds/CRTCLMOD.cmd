/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTCLMOD: compile CL source into an ILE module. */
             CMD        PROMPT('Create CL Module') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(CRTCLMOD)
             PARM       KWD(MODULE) TYPE(Q1) MIN(1) PROMPT('Module')
             PARM       KWD(SRCFILE) TYPE(Q2) PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*MODULE) +
                          SPCVAL((*MODULE)) PROMPT('Source member')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SRCMBRTXT) +
                          SPCVAL((*SRCMBRTXT) (*BLANK ' ')) +
                          PROMPT('Description')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*PRINT) VALUES(*PRINT *NONE) +
                          PMTCTL(*PMTRQS) PROMPT('Output')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*GEN *NOXREF *NOSECLVL *NOEVENTF) MAX(4) +
                          VALUES(*GEN *NOGEN *XREF *NOXREF *SECLVL +
                          *NOSECLVL *EVENTF *NOEVENTF) PMTCTL(*PMTRQS) +
                          PROMPT('Source listing options')
             PARM       KWD(OPTIMIZE) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *BASIC *FULL) +
                          PMTCTL(*PMTRQS) PROMPT('Optimize')
             PARM       KWD(DBGVIEW) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*STMT) VALUES(*STMT *SOURCE *LIST *ALL +
                          *NONE) PMTCTL(*PMTRQS) PROMPT('Debugging view')
             PARM       KWD(DBGENCKEY) TYPE(*CHAR) LEN(16) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) PMTCTL(*PMTRQS) +
                          PROMPT('Debug encryption key')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Replace module')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             PARM       KWD(LOG) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*JOB) +
                          VALUES(*JOB *YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Log commands')
             PARM       KWD(ALWRTVSRC) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Allow RTVCLSRC')
             PARM       KWD(INCFILE) TYPE(Q3) DFT(*SRCFILE) +
                          SNGVAL((*SRCFILE)) PMTCTL(*PMTRQS) +
                          PROMPT('Include file')
             PARM       KWD(TGTRLS) TYPE(*CHAR) LEN(8) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*PRV)) PMTCTL(*PMTRQS) +
                          PROMPT('Target release')
             PARM       KWD(SRTSEQ) TYPE(Q3) DFT(*HEX) SNGVAL((*HEX) +
                          (*JOB) (*JOBRUN) (*LANGIDUNQ) (*LANGIDSHR)) +
                          PMTCTL(*PMTRQS) PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*NAME) LEN(3) DFT(*JOBRUN) +
                          SPCVAL((*JOBRUN) (*JOB)) PMTCTL(*PMTRQS) +
                          PROMPT('Language ID')
             PARM       KWD(ENBPFRCOL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*PEP) VALUES(*PEP *ENTRYEXIT *FULL) +
                          PMTCTL(*PMTRQS) PROMPT('Performance collection')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) DFT(QCLSRC)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
