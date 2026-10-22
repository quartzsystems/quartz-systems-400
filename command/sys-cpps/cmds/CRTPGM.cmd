/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTPGM: bind modules and service programs into an ILE program. */
             CMD        PROMPT('Create Program') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(CRTPGM)
             PARM       KWD(PGM) TYPE(Q1) MIN(1) PROMPT('Program')
             PARM       KWD(MODULE) TYPE(Q2) MAX(300) DFT(*PGM) +
                          SNGVAL((*PGM)) PROMPT('Module')
             PARM       KWD(ENTMOD) TYPE(Q3) DFT(*FIRST) SNGVAL((*FIRST) +
                          (*ONLY) (*PGM)) PROMPT('Program entry module')
             PARM       KWD(BNDSRVPGM) TYPE(E1) MAX(300) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Bind service program')
             PARM       KWD(BNDDIR) TYPE(Q5) MAX(300) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Binding directory')
             PARM       KWD(ACTGRP) TYPE(*NAME) LEN(10) DFT(*NEW) +
                          SPCVAL((*NEW) (*CALLER)) PROMPT('Activation group')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*GEN *NODUPPROC *NODUPVAR *WARN *RSLVREF) +
                          MAX(5) VALUES(*GEN *NOGEN *NODUPPROC *DUPPROC +
                          *NODUPVAR *DUPVAR *WARN *NOWARN *RSLVREF +
                          *UNRSLVREF) PMTCTL(*PMTRQS) +
                          PROMPT('Creation options')
             PARM       KWD(DETAIL) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *BASIC *EXTENDED *FULL) +
                          PMTCTL(*PMTRQS) PROMPT('Listing detail')
             PARM       KWD(ALWUPD) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Allow update')
             PARM       KWD(ALWLIBUPD) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Allow bound *SRVPGM lib update')
             PARM       KWD(USRPRF) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*USER) VALUES(*USER *OWNER) +
                          PMTCTL(*PMTRQS) PROMPT('User profile')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Replace program')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*ENTMODTXT) +
                          SPCVAL((*ENTMODTXT) (*BLANK ' ')) +
                          PMTCTL(*PMTRQS) PROMPT('Description')
             PARM       KWD(TGTRLS) TYPE(*CHAR) LEN(8) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*PRV)) PMTCTL(*PMTRQS) +
                          PROMPT('Target release')
             PARM       KWD(ALWRINZ) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Allow reinitialization')
             PARM       KWD(STGMDL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SNGLVL) VALUES(*SNGLVL *TERASPACE +
                          *INHERIT) PMTCTL(*PMTRQS) PROMPT('Storage model')
             PARM       KWD(IPA) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*NO) +
                          VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Interprocedural analysis')
             PARM       KWD(IPACTLFILE) TYPE(*PNAME) LEN(5000) +
                          DFT(*NONE) SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('IPA control file')
             PARM       KWD(ARGOPT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Argument optimization')
             PARM       KWD(PRFDTA) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*NOCOL) VALUES(*NOCOL *COL) +
                          PMTCTL(*PMTRQS) PROMPT('Profiling data')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL)) +
                          PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL)) +
                          PROMPT('Library')
             Q4:        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL)) +
                          PROMPT('Library')
             Q5:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL)) +
                          PROMPT('Library')
             E1:        ELEM       TYPE(Q4) PROMPT('Service program')
                        ELEM       TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*IMMED) VALUES(*IMMED *DEFER) +
                          PROMPT('Activation')
