/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGUSRPRF: change a user profile; every attribute defaults to *SAME. */
             CMD        PROMPT('Change User Profile') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(CHGUSRPRF)
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('User profile')
             PARM       KWD(PASSWORD) TYPE(*CHAR) LEN(128) DFT(*SAME) +
                          SPCVAL((*SAME) (*USRPRF) (*NONE)) CASE(*MIXED) +
                          INLPMTLEN(*PWD) DSPINPUT(*NO) +
                          PROMPT('User password')
             PARM       KWD(PWDEXP) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Set password to expired')
             PARM       KWD(STATUS) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ENABLED *DISABLED) +
                          PROMPT('Status')
             PARM       KWD(USRCLS) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *USER *SYSOPR *PGMR +
                          *SECADM *SECOFR) PROMPT('User class')
             PARM       KWD(ASTLVL) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *BASIC +
                          *INTERMED *ADVANCED) PROMPT('Assistance level')
             PARM       KWD(CURLIB) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*CRTDFT)) +
                          PROMPT('Current library')
             PARM       KWD(INLPGM) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) +
                          PROMPT('Initial program to call')
             PARM       KWD(INLMNU) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*SIGNOFF)) PROMPT('Initial menu')
             PARM       KWD(LMTCPB) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *PARTIAL *YES) +
                          PROMPT('Limit capabilities')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Description')
             PARM       KWD(SPCAUT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          MAX(8) DFT(*SAME) SNGVAL((*SAME) (*USRCLS) +
                          (*NONE)) VALUES(*ALLOBJ *AUDIT *JOBCTL +
                          *SAVSYS *IOSYSCFG *SECADM *SERVICE *SPLCTL) +
                          PMTCTL(*PMTRQS) PROMPT('Special authority')
             PARM       KWD(SPCENV) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *NONE *S36) +
                          PMTCTL(*PMTRQS) PROMPT('Special environment')
             PARM       KWD(DSPSGNINF) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Show sign-on information')
             PARM       KWD(PWDEXPITV) TYPE(*DEC) LEN(3) DFT(*SAME) +
                          RANGE(1 366) SPCVAL((*SAME -2) (*SYSVAL 0) +
                          (*NOMAX -1)) PMTCTL(*PMTRQS) +
                          PROMPT('Password expiration interval')
             PARM       KWD(PWDCHGBLK) TYPE(*DEC) LEN(2) DFT(*SAME) +
                          RANGE(1 99) SPCVAL((*SAME -2) (*SYSVAL -1) +
                          (*NONE 0)) PMTCTL(*PMTRQS) +
                          PROMPT('Block password change')
             PARM       KWD(LCLPWDMGT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PMTCTL(*PMTRQS) PROMPT('Local password management')
             PARM       KWD(LMTDEVSSN) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *NO *YES 0 1 +
                          2 3 4 5 6 7 8 9) PMTCTL(*PMTRQS) +
                          PROMPT('Limit device sessions')
             PARM       KWD(KBDBUF) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *NO +
                          *TYPEAHEAD *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Keyboard buffering')
             PARM       KWD(MAXSTG) TYPE(*DEC) LEN(11) DFT(*SAME) +
                          RANGE(1 2147483647) SPCVAL((*SAME -2) +
                          (*NOMAX -1)) PMTCTL(*PMTRQS) +
                          PROMPT('Maximum storage (K)')
             PARM       KWD(PTYLMT) TYPE(*INT2) DFT(*SAME) RANGE(0 9) +
                          SPCVAL((*SAME -1)) PMTCTL(*PMTRQS) +
                          PROMPT('Highest schedule priority')
             PARM       KWD(JOBD) TYPE(Q1) DFT(*SAME) SNGVAL((*SAME)) +
                          PMTCTL(*PMTRQS) PROMPT('Job description')
             PARM       KWD(GRPPRF) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Group profile')
             PARM       KWD(OWNER) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *USRPRF *GRPPRF) +
                          PMTCTL(*PMTRQS) PROMPT('Owner of new objects')
             PARM       KWD(GRPAUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *ALL *CHANGE +
                          *USE *EXCLUDE) PMTCTL(*PMTRQS) +
                          PROMPT('Group authority')
             PARM       KWD(GRPAUTTYP) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *PRIVATE *PGP) +
                          PMTCTL(*PMTRQS) PROMPT('Group authority type')
             PARM       KWD(SUPGRPPRF) TYPE(*NAME) LEN(10) MAX(15) +
                          DFT(*SAME) SNGVAL((*SAME) (*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Supplemental groups')
             PARM       KWD(ACGCDE) TYPE(*CHAR) LEN(15) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Accounting code')
             PARM       KWD(DOCPWD) TYPE(*CHAR) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) DSPINPUT(*NO) +
                          PMTCTL(*PMTRQS) PROMPT('Document password')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*USRPRF)) PMTCTL(*PMTRQS) +
                          PROMPT('Message queue')
             PARM       KWD(DLVRY) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NOTIFY *HOLD *BREAK +
                          *DFT) PMTCTL(*PMTRQS) PROMPT('Delivery')
             PARM       KWD(SEV) TYPE(*INT2) DFT(*SAME) RANGE(0 99) +
                          SPCVAL((*SAME -1)) PMTCTL(*PMTRQS) +
                          PROMPT('Severity code filter')
             PARM       KWD(PRTDEV) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*WRKSTN) (*SYSVAL)) +
                          PMTCTL(*PMTRQS) PROMPT('Print device')
             PARM       KWD(OUTQ) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*WRKSTN) (*DEV)) +
                          PMTCTL(*PMTRQS) PROMPT('Output queue')
             PARM       KWD(ATNPGM) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*SYSVAL) (*NONE) (*ASSIST)) +
                          PMTCTL(*PMTRQS) PROMPT('Attention program')
             PARM       KWD(SRTSEQ) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*SYSVAL) (*HEX) (*LANGIDSHR) +
                          (*LANGIDUNQ)) PMTCTL(*PMTRQS) +
                          PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*CHAR) LEN(3) DFT(*SAME) +
                          SPCVAL((*SAME) (*SYSVAL)) PMTCTL(*PMTRQS) +
                          PROMPT('Language ID')
             PARM       KWD(CNTRYID) TYPE(*CHAR) LEN(2) DFT(*SAME) +
                          SPCVAL((*SAME) (*SYSVAL)) PMTCTL(*PMTRQS) +
                          PROMPT('Country or region ID')
             PARM       KWD(CCSID) TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(1 65535) SPCVAL((*SAME -1) (*SYSVAL 0) +
                          (*HEX 65535)) PMTCTL(*PMTRQS) +
                          PROMPT('Coded character set ID')
             PARM       KWD(CHRIDCTL) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *DEVD +
                          *JOBCCSID) PMTCTL(*PMTRQS) +
                          PROMPT('Character identifier control')
             PARM       KWD(SETJOBATR) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          MAX(6) DFT(*SAME) SNGVAL((*SAME) (*SYSVAL) +
                          (*NONE)) VALUES(*CCSID *DATFMT *DATSEP +
                          *DECFMT *SRTSEQ *TIMSEP) PMTCTL(*PMTRQS) +
                          PROMPT('Locale job attributes')
             PARM       KWD(LOCALE) TYPE(*PNAME) LEN(2048) DFT(*SAME) +
                          SPCVAL((*SAME) (*SYSVAL) (*NONE) (*C) (*POSIX)) +
                          CASE(*MIXED) PMTCTL(*PMTRQS) PROMPT('Locale')
             PARM       KWD(USROPT) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          MAX(7) DFT(*SAME) SNGVAL((*SAME) (*NONE)) +
                          VALUES(*CLKWD *EXPERT *ROLLKEY *NOSTSMSG +
                          *STSMSG *HLPFULL *PRTMSG) PMTCTL(*PMTRQS) +
                          PROMPT('User options')
             PARM       KWD(UID) TYPE(*DEC) LEN(10) DFT(*SAME) +
                          RANGE(0 4294967294) SPCVAL((*SAME -1)) +
                          PMTCTL(*PMTRQS) PROMPT('User ID number')
             PARM       KWD(GID) TYPE(*DEC) LEN(10) DFT(*SAME) +
                          RANGE(1 4294967294) SPCVAL((*SAME -2) (*NONE 0) +
                          (*GEN -1)) PMTCTL(*PMTRQS) +
                          PROMPT('Group ID number')
             PARM       KWD(HOMEDIR) TYPE(*PNAME) LEN(5000) DFT(*SAME) +
                          SPCVAL((*SAME) (*USRPRF)) CASE(*MIXED) +
                          PMTCTL(*PMTRQS) PROMPT('Home directory')
             PARM       KWD(EIMASSOC) TYPE(E1) DFT(*NOCHG) +
                          SNGVAL((*NOCHG)) PMTCTL(*PMTRQS) +
                          PROMPT('EIM association')
             PARM       KWD(USREXPDATE) TYPE(*DATE) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE) (*USREXPITV)) +
                          PMTCTL(*PMTRQS) PROMPT('User expiration date')
             PARM       KWD(USREXPITV) TYPE(*DEC) LEN(3) DFT(*SAME) +
                          RANGE(1 366) SPCVAL((*SAME -1) (*NONE 0)) +
                          PMTCTL(*PMTRQS) PROMPT('User expiration interval')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
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
