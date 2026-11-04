/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTRPGPGM: compile RPG III (RPG/400) source into a program           */
/* (licensed program QS00RPG, docs/RPG3.md).                            */
             CMD        PROMPT('Create RPG/400 Program') MAXPOS(4) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(CRTRPGPGM)
             PARM       KWD(PGM) TYPE(Q1) DFT(*CTLSPEC) +
                          SNGVAL((*CTLSPEC)) PROMPT('Program')
             PARM       KWD(SRCFILE) TYPE(Q2) PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*PGM) +
                          SPCVAL((*PGM)) PROMPT('Source member')
             PARM       KWD(GENLVL) TYPE(*DEC) LEN(2 0) DFT(9) +
                          RANGE(0 99) PROMPT('Generation severity level')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SRCMBRTXT) +
                          SPCVAL((*SRCMBRTXT) (*BLANK ' ')) +
                          PROMPT('Text ''description''')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          MAX(7) VALUES(*SOURCE +
                          *NOSOURCE *SRC *NOSRC *XREF *NOXREF *GEN +
                          *NOGEN *DUMP *NODUMP *SECLVL *NOSECLVL +
                          *SRCDBG *NOSRCDBG *LSTDBG *NOLSTDBG) +
                          PMTCTL(*PMTRQS) PROMPT('Source listing options')
             PARM       KWD(GENOPT) TYPE(*CHAR) LEN(11) RSTD(*YES) +
                          MAX(7) VALUES(*LIST *NOLIST +
                          *XREF *NOXREF *ATR *NOATR *DUMP *NODUMP +
                          *PATCH *NOPATCH *OPTIMIZE *NOOPTIMIZE) +
                          PMTCTL(*PMTRQS) PROMPT('Generation options')
             PARM       KWD(INDENT) TYPE(*CHAR) LEN(5) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Source listing indentation')
             PARM       KWD(CVTOPT) TYPE(*CHAR) LEN(11) RSTD(*YES) +
                          DFT(*NONE) SNGVAL((*NONE)) MAX(4) +
                          VALUES(*DATETIME *NODATETIME *VARCHAR +
                          *NOVARCHAR *GRAPHIC *NOGRAPHIC) +
                          PMTCTL(*PMTRQS) PROMPT('Type conversion options')
             PARM       KWD(SRTSEQ) TYPE(Q3) DFT(*HEX) SNGVAL((*HEX) +
                          (*JOB) (*JOBRUN) (*LANGIDUNQ) (*LANGIDSHR)) +
                          PMTCTL(*PMTRQS) PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*NAME) LEN(7) DFT(*JOBRUN) +
                          SPCVAL((*JOBRUN) (*JOB)) PMTCTL(*PMTRQS) +
                          PROMPT('Language identifier')
             PARM       KWD(SAAFLAG) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NOFLAG) VALUES(*NOFLAG *FLAG) +
                          PMTCTL(*PMTRQS) PROMPT('SAA flagging')
             PARM       KWD(PRTFILE) TYPE(Q4) PMTCTL(*PMTRQS) +
                          PROMPT('Print file')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Replace program')
             PARM       KWD(TGTRLS) TYPE(*CHAR) LEN(8) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*PRV)) PMTCTL(*PMTRQS) +
                          PROMPT('Target release')
             PARM       KWD(USRPRF) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*USER) VALUES(*USER *OWNER) +
                          PMTCTL(*PMTRQS) PROMPT('User profile')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             PARM       KWD(PHSTRC) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Phase trace')
             PARM       KWD(ITDUMP) TYPE(*CHAR) LEN(5) DFT(*NONE) +
                          SPCVAL((*NONE)) MAX(10) PMTCTL(*PMTRQS) +
                          PROMPT('Intermediate text dump')
             PARM       KWD(SNPDUMP) TYPE(*CHAR) LEN(5) DFT(*NONE) +
                          SPCVAL((*NONE)) MAX(10) PMTCTL(*PMTRQS) +
                          PROMPT('Snap dump')
             PARM       KWD(CODELIST) TYPE(*CHAR) LEN(5) DFT(*NONE) +
                          SPCVAL((*NONE) (*ALL)) MAX(10) +
                          PMTCTL(*PMTRQS) PROMPT('Codelist')
             PARM       KWD(IGNDECERR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Ignore decimal data error')
             PARM       KWD(ALWNULL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Allow null values')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) DFT(QRPGSRC)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q4:        QUAL       TYPE(*NAME) LEN(10) DFT(QSYSPRT)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
