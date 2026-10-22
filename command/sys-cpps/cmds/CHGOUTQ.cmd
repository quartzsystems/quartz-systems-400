/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGOUTQ: change the attributes of an output queue. */
             CMD        PROMPT('Change Output Queue') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSPCMD) HLPID(CHGOUTQ)
             PARM       KWD(OUTQ) TYPE(Q1) MIN(1) PROMPT('Output queue')
             PARM       KWD(MAXPAGES) TYPE(E1) MAX(5) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) +
                          PROMPT('Maximum spooled file size')
             PARM       KWD(AUTOSTRWTR) TYPE(*INT2) DFT(*SAME) +
                          RANGE(1 10) SPCVAL((*SAME -1) (*NONE 0)) +
                          PROMPT('Writers to autostart')
             PARM       KWD(DSPDTA) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES *OWNER) +
                          PROMPT('Display any file')
             PARM       KWD(JOBSEP) TYPE(*INT2) DFT(*SAME) RANGE(0 9) +
                          SPCVAL((*SAME -2) (*MSG -1)) +
                          PROMPT('Job separators')
             PARM       KWD(OPRCTL) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Operator controlled')
             PARM       KWD(DTAQ) TYPE(Q2) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) PROMPT('Data queue')
             PARM       KWD(AUTCHK) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *OWNER *DTAAUT) +
                          PROMPT('Authority to check')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Description')
             PARM       KWD(SEQ) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *FIFO *JOBNBR) +
                          PMTCTL(*PMTRQS) PROMPT('Order of files on queue')
             PARM       KWD(RMTSYS) TYPE(*CHAR) LEN(255) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE) (*INTNETADR)) +
                          PMTCTL(*PMTRQS) PROMPT('Remote system')
             PARM       KWD(RMTPRTQ) TYPE(*CHAR) LEN(255) DFT(*SAME) +
                          SPCVAL((*SAME) (*USER) (*SYSTEM)) CASE(*MIXED) +
                          PMTCTL(*PMTRQS) PROMPT('Remote printer queue')
             PARM       KWD(CNNTYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SNA *IP *IPX *USRDFN) +
                          PMTCTL(*PMTRQS) PROMPT('Connection type')
             PARM       KWD(DESTTYPE) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *OTHER *OS400 +
                          *OS400V2 *PSF2 *S390 *NDS *NETWARE3 +
                          *NETWARE4) PMTCTL(*PMTRQS) +
                          PROMPT('Destination type')
             PARM       KWD(VMMVSCLASS) TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME)) PMTCTL(*PMTRQS) +
                          PROMPT('VM/MVS class')
             PARM       KWD(FCB) TYPE(*NAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE) (*USRDTA)) +
                          PMTCTL(*PMTRQS) PROMPT('Forms control buffer')
             PARM       KWD(TRANSFORM) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PMTCTL(*PMTRQS) PROMPT('Transform SCS to ASCII')
             PARM       KWD(MFRTYPMDL) TYPE(*CHAR) LEN(17) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE) (*WSCST) (*IBM42011) +
                          (*HPLJ4) (*HP4) (*HP5SI) (*IBM3812) +
                          (*IBM4019) (*IBM4029) (*IBM4039HP) +
                          (*IBM5577) (*IBMPAGES) (*LEXOPTRA) +
                          (*NECP2) (*PRESTIGE)) PMTCTL(*PMTRQS) +
                          PROMPT('Manufacturer type and model')
             PARM       KWD(WSCST) TYPE(Q2) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Workstation customizing')
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(45) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Internet address')
             PARM       KWD(DESTOPT) TYPE(*CHAR) LEN(128) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) CASE(*MIXED) +
                          PMTCTL(*PMTRQS) PROMPT('Destination options')
             PARM       KWD(USRDFNOPT) TYPE(*CHAR) LEN(10) MAX(4) +
                          DFT(*SAME) SNGVAL((*SAME) (*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('User defined options')
             PARM       KWD(USRDFNDTA) TYPE(*CHAR) LEN(255) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('User defined data')
             PARM       KWD(USRDFNOBJ) TYPE(E2) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('User defined object')
             PARM       KWD(USRDTATFM) TYPE(Q2) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('User data transform')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT4) RANGE(1 999999) MIN(1) +
                          PROMPT('Pages')
                        ELEM       TYPE(*TIME) DFT(000000) +
                          PROMPT('Start time')
                        ELEM       TYPE(*TIME) DFT(235959) +
                          PROMPT('End time')
             E2:        ELEM       TYPE(Q2) PROMPT('Object')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          VALUES(*DTAARA *DTAQ *FILE *PSFCFG *USRIDX +
                          *USRQ *USRSPC) PROMPT('Object type')
