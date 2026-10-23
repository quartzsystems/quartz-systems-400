/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTCMD: compile a command definition source into a *CMD object. */
             CMD        PROMPT('Create Command') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(CRTCMD)
             PARM       KWD(CMD) TYPE(Q1) MIN(1) PROMPT('Command')
             PARM       KWD(PGM) TYPE(Q2) MIN(1) SNGVAL((*REXX)) +
                          PROMPT('Program to process command')
             PARM       KWD(SRCFILE) TYPE(Q3) PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*CMD) +
                          SPCVAL((*CMD)) PROMPT('Source member')
             PARM       KWD(REXSRCFILE) TYPE(Q4) PMTCTL(P1) +
                          PROMPT('REXX source file')
             PARM       KWD(REXSRCMBR) TYPE(*NAME) LEN(10) DFT(*CMD) +
                          SPCVAL((*CMD)) PMTCTL(P1) +
                          PROMPT('REXX source member')
             PARM       KWD(REXCMDENV) TYPE(Q5) DFT(*COMMAND) +
                          SNGVAL((*COMMAND) (*CPICOMM) (*EXECSQL)) +
                          PMTCTL(P1) +
                          PROMPT('REXX command environment')
             PARM       KWD(THDSAFE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *COND *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Threadsafe')
             PARM       KWD(MLTTHDACN) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *RUN *MSG *NORUN) +
                          PMTCTL(*PMTRQS) PROMPT('Multithreaded job action')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SRCMBRTXT) +
                          SPCVAL((*SRCMBRTXT) (*BLANK ' ')) +
                          PROMPT('Description')
             PARM       KWD(VLDCKR) TYPE(Q6) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Validity checking program')
             PARM       KWD(MODE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *PROD *DEBUG *SERVICE) +
                          MAX(3) PMTCTL(*PMTRQS) PROMPT('Mode in which valid')
             PARM       KWD(ALLOW) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *BATCH *INTERACT *BPGM +
                          *IPGM *BREXX *IREXX *EXEC *BMOD *IMOD) MAX(9) +
                          PMTCTL(*PMTRQS) PROMPT('Where allowed to run')
             PARM       KWD(ALWLMTUSR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Allow limited users')
             PARM       KWD(MAXPOS) TYPE(*INT2) DFT(*NONE) RANGE(0 99) +
                          SPCVAL((*NONE -1)) PMTCTL(*PMTRQS) +
                          PROMPT('Max positional parameters')
             PARM       KWD(PMTFILE) TYPE(E1) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Prompt file')
             PARM       KWD(MSGF) TYPE(Q7) PMTCTL(*PMTRQS) +
                          PROMPT('Message file')
             PARM       KWD(HLPPNLGRP) TYPE(Q8) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Help panel group')
             PARM       KWD(HLPID) TYPE(*CHAR) LEN(32) DFT(*NONE) +
                          SPCVAL((*NONE) (*CMD)) PMTCTL(*PMTRQS) +
                          PROMPT('Help identifier')
             PARM       KWD(HLPSCHIDX) TYPE(Q9) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Help search index')
             PARM       KWD(CURLIB) TYPE(*NAME) LEN(10) DFT(*NOCHG) +
                          SPCVAL((*NOCHG) (*CRTDFT)) PMTCTL(*PMTRQS) +
                          PROMPT('Current library')
             PARM       KWD(PRDLIB) TYPE(*NAME) LEN(10) DFT(*NOCHG) +
                          SPCVAL((*NOCHG) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Product library')
             PARM       KWD(PMTOVRPGM) TYPE(Q6) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Prompt override program')
             PARM       KWD(ENBGUI) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Enable GUI')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Replace command')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10) DFT(QCMDSRC)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q4:        QUAL       TYPE(*NAME) LEN(10) DFT(QREXSRC)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q5:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q6:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q7:        QUAL       TYPE(*NAME) LEN(10) DFT(QCPFMSG)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q8:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q9:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(Q8) PROMPT('File')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*STATIC) VALUES(*STATIC *DYNAMIC) +
                          PROMPT('Message retrieval')
             P1:        PMTCTL     CTL(PGM) COND((*EQ *REXX)) +
                          NBRTRUE(*ALL)
             DEP        CTL((&THDSAFE *NE *COND)) PARM(&MLTTHDACN) +
                          NBRTRUE(*NONE) MSGID(CPD6278)
