/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTBNDCL: compile CL source and bind it into an ILE program in one */
/* step.                                                               */
             CMD        PROMPT('Create Bound CL Program') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(CRTBNDCL)
             PARM       KWD(PGM) TYPE(Q1) MIN(1) PROMPT('Program')
             PARM       KWD(SRCFILE) TYPE(Q2) PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*PGM) +
                          SPCVAL((*PGM)) PROMPT('Source member')
             PARM       KWD(SRCSTMF) TYPE(*PNAME) LEN(5000) +
                          CASE(*MIXED) PROMPT('Source stream file')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SRCMBRTXT) +
                          SPCVAL((*SRCMBRTXT) (*BLANK ' ')) +
                          PROMPT('Description')
             PARM       KWD(DFTACTGRP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Default activation group')
             PARM       KWD(ACTGRP) TYPE(*NAME) LEN(10) DFT(*STGMDL) +
                          SPCVAL((*STGMDL) (*NEW) (*CALLER)) +
                          PMTCTL(*PMTRQS) PROMPT('Activation group')
             PARM       KWD(STGMDL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SNGLVL) VALUES(*SNGLVL *TERASPACE +
                          *INHERIT) PMTCTL(*PMTRQS) PROMPT('Storage model')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*PRINT) VALUES(*PRINT *NONE) +
                          PMTCTL(*PMTRQS) PROMPT('Output')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*GEN *NOXREF *NOSECLVL *NOEVENTF) MAX(4) +
                          VALUES(*GEN *NOGEN *XREF *NOXREF *SECLVL +
                          *NOSECLVL *EVENTF *NOEVENTF) PMTCTL(*PMTRQS) +
                          PROMPT('Source listing options')
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
             PARM       KWD(INCFILE) TYPE(Q3) DFT(*SRCFILE) +
                          SNGVAL((*SRCFILE)) PMTCTL(*PMTRQS) +
                          PROMPT('Include file')
             PARM       KWD(OPTIMIZE) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *BASIC *FULL) +
                          PMTCTL(*PMTRQS) PROMPT('Optimize')
             PARM       KWD(DBGVIEW) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*STMT) VALUES(*STMT *SOURCE *LIST *ALL +
                          *NONE) PMTCTL(*PMTRQS) PROMPT('Debugging view')
             PARM       KWD(DBGENCKEY) TYPE(*CHAR) LEN(16) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) PMTCTL(*PMTRQS) +
                          PROMPT('Debug encryption key')
             PARM       KWD(ENBPFRCOL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*PEP) VALUES(*PEP *ENTRYEXIT *FULL) +
                          PMTCTL(*PMTRQS) PROMPT('Performance collection')
             PARM       KWD(BNDDIR) TYPE(Q4) MAX(300) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Binding directory')
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
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL)) +
                          PROMPT('Library')
             DEP        CTL((&DFTACTGRP *EQ *YES)) PARM(&ACTGRP) +
                          NBRTRUE(*NONE) MSGID(CPD0875)
