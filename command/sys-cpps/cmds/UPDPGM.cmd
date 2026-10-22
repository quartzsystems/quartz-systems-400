/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* UPDPGM: replace modules of an ILE program and rebind it. */
             CMD        PROMPT('Update Program') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(UPDPGM)
             PARM       KWD(PGM) TYPE(Q1) MIN(1) PROMPT('Program')
             PARM       KWD(MODULE) TYPE(Q2) MAX(300) DFT(*NONE) +
                          SNGVAL((*NONE) (*ALL)) PROMPT('Module')
             PARM       KWD(RPLLIB) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ONLY) VALUES(*ONLY *FIRST *MODULE) +
                          PROMPT('Library to replace from')
             PARM       KWD(BNDSRVPGM) TYPE(E1) MAX(300) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Bind service program')
             PARM       KWD(BNDDIR) TYPE(Q5) MAX(300) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Binding directory')
             PARM       KWD(ACTGRP) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*NEW) (*CALLER)) +
                          PMTCTL(*PMTRQS) PROMPT('Activation group')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*GEN *NODUPPROC *NODUPVAR *WARN *RSLVREF) +
                          MAX(5) VALUES(*GEN *NOGEN *NODUPPROC *DUPPROC +
                          *NODUPVAR *DUPVAR *WARN *NOWARN *RSLVREF +
                          *UNRSLVREF) PMTCTL(*PMTRQS) +
                          PROMPT('Creation options')
             PARM       KWD(DETAIL) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *BASIC *EXTENDED *FULL) +
                          PMTCTL(*PMTRQS) PROMPT('Listing detail')
             PARM       KWD(ALWUPD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PMTCTL(*PMTRQS) PROMPT('Allow update')
             PARM       KWD(ALWLIBUPD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Allow bound *SRVPGM lib update')
             PARM       KWD(USRPRF) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *USER *OWNER) +
                          PMTCTL(*PMTRQS) PROMPT('User profile')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Description')
             PARM       KWD(ALWRINZ) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Allow reinitialization')
             PARM       KWD(ARGOPT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Argument optimization')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*GENERIC) LEN(10)
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
