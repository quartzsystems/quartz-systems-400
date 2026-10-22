/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTUSRPRF: create a user profile. */
             CMD        PROMPT('Create User Profile') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(CRTUSRPRF)
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('User profile')
             PARM       KWD(PASSWORD) TYPE(*CHAR) LEN(128) DFT(*USRPRF) +
                          SPCVAL((*USRPRF) (*NONE)) CASE(*MIXED) +
                          INLPMTLEN(*PWD) DSPINPUT(*NO) +
                          PROMPT('User password')
             PARM       KWD(PWDEXP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Set password to expired')
             PARM       KWD(STATUS) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*ENABLED) VALUES(*ENABLED *DISABLED) +
                          PROMPT('Status')
             PARM       KWD(USRCLS) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*USER) VALUES(*USER *SYSOPR *PGMR +
                          *SECADM *SECOFR) PROMPT('User class')
             PARM       KWD(ASTLVL) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *BASIC *INTERMED +
                          *ADVANCED) PROMPT('Assistance level')
             PARM       KWD(CURLIB) TYPE(*NAME) LEN(10) DFT(*CRTDFT) +
                          SPCVAL((*CRTDFT)) PROMPT('Current library')
             PARM       KWD(INLPGM) TYPE(Q1) DFT(*NONE) SNGVAL((*NONE)) +
                          PROMPT('Initial program to call')
             PARM       KWD(INLMNU) TYPE(Q2) DFT(MAIN) +
                          SNGVAL((*SIGNOFF)) PROMPT('Initial menu')
             PARM       KWD(LMTCPB) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *PARTIAL *YES) +
                          PROMPT('Limit capabilities')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(SPCAUT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          MAX(8) DFT(*USRCLS) SNGVAL((*USRCLS) (*NONE)) +
                          VALUES(*ALLOBJ *AUDIT *JOBCTL *SAVSYS +
                          *IOSYSCFG *SECADM *SERVICE *SPLCTL) +
                          PMTCTL(*PMTRQS) PROMPT('Special authority')
             PARM       KWD(SPCENV) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *NONE *S36) +
                          PMTCTL(*PMTRQS) PROMPT('Special environment')
             PARM       KWD(DSPSGNINF) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Show sign-on information')
             PARM       KWD(PWDEXPITV) TYPE(*DEC) LEN(3) DFT(*SYSVAL) +
                          RANGE(1 366) SPCVAL((*SYSVAL 0) (*NOMAX -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Password expiration interval')
             PARM       KWD(PWDCHGBLK) TYPE(*DEC) LEN(2) DFT(*SYSVAL) +
                          RANGE(1 99) SPCVAL((*SYSVAL -1) (*NONE 0)) +
                          PMTCTL(*PMTRQS) PROMPT('Block password change')
             PARM       KWD(LCLPWDMGT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Local password management')
             PARM       KWD(LMTDEVSSN) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *NO *YES 0 1 2 3 +
                          4 5 6 7 8 9) PMTCTL(*PMTRQS) +
                          PROMPT('Limit device sessions')
             PARM       KWD(KBDBUF) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *NO *TYPEAHEAD +
                          *YES) PMTCTL(*PMTRQS) PROMPT('Keyboard buffering')
             PARM       KWD(MAXSTG) TYPE(*DEC) LEN(11) DFT(*NOMAX) +
                          RANGE(1 2147483647) SPCVAL((*NOMAX -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum storage (K)')
             PARM       KWD(PTYLMT) TYPE(*INT2) DFT(3) RANGE(0 9) +
                          PMTCTL(*PMTRQS) PROMPT('Highest schedule priority')
             PARM       KWD(JOBD) TYPE(Q3) PMTCTL(*PMTRQS) +
                          PROMPT('Job description')
             PARM       KWD(GRPPRF) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Group profile')
             PARM       KWD(OWNER) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*USRPRF) VALUES(*USRPRF *GRPPRF) +
                          PMTCTL(*PMTRQS) PROMPT('Owner of new objects')
             PARM       KWD(GRPAUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL *CHANGE *USE +
                          *EXCLUDE) PMTCTL(*PMTRQS) +
                          PROMPT('Group authority')
             PARM       KWD(GRPAUTTYP) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*PRIVATE) VALUES(*PRIVATE *PGP) +
                          PMTCTL(*PMTRQS) PROMPT('Group authority type')
             PARM       KWD(SUPGRPPRF) TYPE(*NAME) LEN(10) MAX(15) +
                          DFT(*NONE) SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Supplemental groups')
             PARM       KWD(ACGCDE) TYPE(*CHAR) LEN(15) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Accounting code')
             PARM       KWD(DOCPWD) TYPE(*CHAR) LEN(8) DFT(*NONE) +
                          SPCVAL((*NONE)) DSPINPUT(*NO) PMTCTL(*PMTRQS) +
                          PROMPT('Document password')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*USRPRF) +
                          SNGVAL((*USRPRF)) PMTCTL(*PMTRQS) +
                          PROMPT('Message queue')
             PARM       KWD(DLVRY) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NOTIFY) VALUES(*NOTIFY *HOLD *BREAK *DFT) +
                          PMTCTL(*PMTRQS) PROMPT('Delivery')
             PARM       KWD(SEV) TYPE(*INT2) DFT(0) RANGE(0 99) +
                          PMTCTL(*PMTRQS) PROMPT('Severity code filter')
             PARM       KWD(PRTDEV) TYPE(*NAME) LEN(10) DFT(*WRKSTN) +
                          SPCVAL((*WRKSTN) (*SYSVAL)) PMTCTL(*PMTRQS) +
                          PROMPT('Print device')
             PARM       KWD(OUTQ) TYPE(Q1) DFT(*WRKSTN) +
                          SNGVAL((*WRKSTN) (*DEV)) PMTCTL(*PMTRQS) +
                          PROMPT('Output queue')
             PARM       KWD(ATNPGM) TYPE(Q1) DFT(*SYSVAL) +
                          SNGVAL((*SYSVAL) (*NONE) (*ASSIST)) +
                          PMTCTL(*PMTRQS) PROMPT('Attention program')
             PARM       KWD(SRTSEQ) TYPE(Q1) DFT(*SYSVAL) +
                          SNGVAL((*SYSVAL) (*HEX) (*LANGIDSHR) +
                          (*LANGIDUNQ)) PMTCTL(*PMTRQS) +
                          PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*CHAR) LEN(3) DFT(*SYSVAL) +
                          SPCVAL((*SYSVAL)) PMTCTL(*PMTRQS) +
                          PROMPT('Language ID')
             PARM       KWD(CNTRYID) TYPE(*CHAR) LEN(2) DFT(*SYSVAL) +
                          SPCVAL((*SYSVAL)) PMTCTL(*PMTRQS) +
                          PROMPT('Country or region ID')
             PARM       KWD(CCSID) TYPE(*DEC) LEN(5) DFT(*SYSVAL) +
                          RANGE(1 65535) SPCVAL((*SYSVAL 0) (*HEX 65535)) +
                          PMTCTL(*PMTRQS) PROMPT('Coded character set ID')
             PARM       KWD(CHRIDCTL) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *DEVD *JOBCCSID) +
                          PMTCTL(*PMTRQS) PROMPT('Character identifier control')
             PARM       KWD(SETJOBATR) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          MAX(6) DFT(*SYSVAL) SNGVAL((*SYSVAL) (*NONE)) +
                          VALUES(*CCSID *DATFMT *DATSEP *DECFMT *SRTSEQ +
                          *TIMSEP) PMTCTL(*PMTRQS) +
                          PROMPT('Locale job attributes')
             PARM       KWD(LOCALE) TYPE(*PNAME) LEN(2048) DFT(*SYSVAL) +
                          SPCVAL((*SYSVAL) (*NONE) (*C) (*POSIX)) +
                          CASE(*MIXED) PMTCTL(*PMTRQS) PROMPT('Locale')
             PARM       KWD(USROPT) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          MAX(7) DFT(*NONE) SNGVAL((*NONE)) +
                          VALUES(*CLKWD *EXPERT *ROLLKEY *NOSTSMSG +
                          *STSMSG *HLPFULL *PRTMSG) PMTCTL(*PMTRQS) +
                          PROMPT('User options')
             PARM       KWD(UID) TYPE(*DEC) LEN(10) DFT(*GEN) +
                          RANGE(0 4294967294) SPCVAL((*GEN -1)) +
                          PMTCTL(*PMTRQS) PROMPT('User ID number')
             PARM       KWD(GID) TYPE(*DEC) LEN(10) DFT(*NONE) +
                          RANGE(1 4294967294) SPCVAL((*NONE 0) (*GEN -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Group ID number')
             PARM       KWD(HOMEDIR) TYPE(*PNAME) LEN(5000) DFT(*USRPRF) +
                          SPCVAL((*USRPRF)) CASE(*MIXED) PMTCTL(*PMTRQS) +
                          PROMPT('Home directory')
             PARM       KWD(EIMASSOC) TYPE(E1) DFT(*NOCHG) +
                          SNGVAL((*NOCHG)) PMTCTL(*PMTRQS) +
                          PROMPT('EIM association')
             PARM       KWD(USREXPDATE) TYPE(*DATE) DFT(*NONE) +
                          SPCVAL((*NONE) (*USREXPITV)) PMTCTL(*PMTRQS) +
                          PROMPT('User expiration date')
             PARM       KWD(USREXPITV) TYPE(*DEC) LEN(3) DFT(*NONE) +
                          RANGE(1 366) SPCVAL((*NONE 0)) PMTCTL(*PMTRQS) +
                          PROMPT('User expiration interval')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*EXCLUDE) +
                          SPCVAL((*EXCLUDE) (*ALL) (*CHANGE) (*USE)) +
                          PMTCTL(*PMTRQS) PROMPT('Public authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) DFT(MAIN)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10) DFT(QDFTJOBD)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(256) DFT(*USRPRF) +
                          SPCVAL((*USRPRF)) CASE(*MIXED) +
                          PROMPT('EIM identifier')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) MAX(4) +
                          DFT(*TARGET) VALUES(*TARGET *SOURCE *TGTSRC +
                          *ADMIN *ALL) PROMPT('Association type')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD *REMOVE) +
                          PROMPT('Association action')
                        ELEM       TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NOCRTEIMID) VALUES(*NOCRTEIMID *CRTEIMID) +
                          PROMPT('Create EIM identifier')
