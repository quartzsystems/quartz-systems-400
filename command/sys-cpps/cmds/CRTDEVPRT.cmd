/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTDEVPRT: create a printer device description. The LAN class      */
/* over TCP/IP is the one that prints here; the device's writer       */
/* reaches the printer by its system driver program.                  */
             CMD        PROMPT('Create Device Desc (Printer)') +
                          MAXPOS(4) ALLOW(*ALL) HLPPNLGRP(QHDCDEV) +
                          HLPID(CRTDEVPRT)
             PARM       KWD(DEVD) TYPE(*NAME) LEN(10) MIN(1) +
                          KEYPARM(*YES) PROMPT('Device description')
             PARM       KWD(DEVCLS) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          MIN(1) VALUES(*LCL *RMT *VRT *SNPT *LAN) +
                          KEYPARM(*YES) PROMPT('Device class')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) MIN(1) +
                          VALUES(3287 3812 4019 4201 4202 4207 4208 +
                          4214 4216 4224 4234 4245 4247 5204 5219 5224 +
                          5225 5256 5262 5553 5583 6252 6404 6408 6412 +
                          *IPDS) KEYPARM(*YES) PROMPT('Device type')
             PARM       KWD(MODEL) TYPE(*CHAR) LEN(6) MIN(1) +
                          KEYPARM(*YES) PROMPT('Device model')
             PARM       KWD(LANATTACH) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*LEXLINK) VALUES(*LEXLINK *IP *USRDFN) +
                          PMTCTL(PL) PROMPT('LAN attachment')
             PARM       KWD(PORT) TYPE(*INT4) RANGE(0 65535) +
                          PMTCTL(PI) PROMPT('Port number')
             PARM       KWD(ONLINE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(PI) +
                          PROMPT('Online at IPL')
             PARM       KWD(PRTQLTY) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*STD) VALUES(*STD *DRAFT *NLQ) +
                          PMTCTL(PN) PROMPT('Print quality')
             PARM       KWD(FONT) TYPE(E1) PMTCTL(PI) PROMPT('Font')
             PARM       KWD(FORMFEED) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*TYPE) VALUES(*TYPE *CONT *CONT2 *CUT +
                          *AUTOCUT) PMTCTL(PI) PROMPT('Form feed')
             PARM       KWD(SEPDRAWER) TYPE(*INT2) DFT(*FILE) +
                          RANGE(1 255) SPCVAL((*FILE 0)) PMTCTL(PI) +
                          PROMPT('Separator drawer')
             PARM       KWD(SEPPGM) TYPE(Q1) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(PI) +
                          PROMPT('Separator program')
             PARM       KWD(PRTERRMSG) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*INQ) VALUES(*INQ *INFO) PMTCTL(PN) +
                          PROMPT('Printer error message')
             PARM       KWD(MSGQ) TYPE(Q2) DFT(*CTLD) +
                          SNGVAL((*CTLD) (*SYSOPR)) PMTCTL(PN) +
                          PROMPT('Message queue')
             PARM       KWD(TRANSFORM) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(PN) +
                          PROMPT('Host print transform')
             PARM       KWD(MFRTYPMDL) TYPE(*CHAR) LEN(15) +
                          DFT(*IBM42011) SPCVAL((*WSCST) (*IBM42011) +
                          (*HP4) (*HP5) (*HP5SI) (*HPII) (*HPIID) +
                          (*HPIIP) (*HPIII) (*HPIIID) (*HPIIIP) +
                          (*HPIIISI) (*HPPLUS) (*HPLJ4) (*IBM3812) +
                          (*IBM4019)) PMTCTL(PT) +
                          PROMPT('Manufacturer type and model')
             PARM       KWD(PPRSRC1) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*MFRTYPMDL) VALUES(*MFRTYPMDL *LETTER +
                          *LEGAL *EXECUTIVE *LEDGER *A3 *A4 *A5 *B4 *B5 +
                          *CONT80 *CONT132) PMTCTL(PT) +
                          PROMPT('Paper source 1')
             PARM       KWD(PPRSRC2) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*MFRTYPMDL) VALUES(*MFRTYPMDL *LETTER +
                          *LEGAL *EXECUTIVE *LEDGER *A3 *A4 *A5 *B4 *B5 +
                          *CONT80 *CONT132) PMTCTL(PT) +
                          PROMPT('Paper source 2')
             PARM       KWD(ENVELOPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*MFRTYPMDL) VALUES(*MFRTYPMDL *MONARCH +
                          *NUMBER9 *NUMBER10 *B5 *C5 *DL *NONE) +
                          PMTCTL(PT) PROMPT('Envelope source')
             PARM       KWD(MAXPNDRQS) TYPE(*INT2) DFT(6) RANGE(1 31) +
                          PMTCTL(PN) PROMPT('Maximum pending requests')
             PARM       KWD(PRTRQSTMR) TYPE(*INT2) DFT(*NOMAX) +
                          RANGE(1 3600) SPCVAL((*NOMAX 0)) PMTCTL(PN) +
                          PROMPT('Print request timer')
             PARM       KWD(CHRID) TYPE(E2) DFT(*SYSVAL) +
                          SNGVAL((*SYSVAL)) PMTCTL(PN) +
                          PROMPT('Character identifier')
             PARM       KWD(RMTLOCNAME) TYPE(*CHAR) LEN(255) +
                          CASE(*MIXED) PMTCTL(PN) +
                          PROMPT('Remote location')
             PARM       KWD(WSCST) TYPE(Q1) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Workstation customizing object')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*CHANGE) +
                          SPCVAL((*CHANGE) (*ALL) (*USE) (*EXCLUDE) +
                          (*LIBCRTAUT)) PMTCTL(*PMTRQS) +
                          PROMPT('Authority')
             PARM       KWD(USRDTATFM) TYPE(Q1) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(PN) +
                          PROMPT('User data transform')
             PARM       KWD(USRDRVPGM) TYPE(Q1) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(PN) +
                          PROMPT('User driver program')
             PARM       KWD(SYSDRVPGM) TYPE(*CHAR) LEN(15) +
                          SPCVAL((*HPPJLDRV) (*IBMPJLDRV) +
                          (*NETSTNDRV) (*IBMSNMPDRV) (*IBMIPPDRV)) +
                          PMTCTL(PN) PROMPT('System driver program')
             PARM       KWD(SECURECNN) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(PN) +
                          PROMPT('Secure connection')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PMTCTL(PI) +
                          PROMPT('Text ''description''')
             PL:        PMTCTL     CTL(DEVCLS) COND((*EQ *LAN))
             PI:        PMTCTL     CTL(LANATTACH) COND((*EQ *IP))
             PN:        PMTCTL     CTL(PORT) COND((*SPCFD))
             PT:        PMTCTL     CTL(TRANSFORM) COND((*EQ *YES))
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) DFT(QSYSOPR)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(10) PROMPT('Identifier')
                        ELEM       TYPE(*DEC) LEN(4 1) DFT(*NONE) +
                          SPCVAL((*NONE 0)) PROMPT('Point size')
             E2:        ELEM       TYPE(*INT2) RANGE(1 32767) MIN(1) +
                          PROMPT('Graphic character set')
                        ELEM       TYPE(*INT2) RANGE(1 32767) MIN(1) +
                          PROMPT('Code page')
