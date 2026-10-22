/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTDEVDSP: create a display device description. */
             CMD        PROMPT('Create Device Desc (Display)') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHCFCMD) +
                          HLPID(CRTDEVDSP)
             PARM       KWD(DEVD) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Device description')
             PARM       KWD(DEVCLS) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          MIN(1) VALUES(*LCL *RMT *VRT *SNPT) +
                          PROMPT('Device class')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) MIN(1) +
                          VALUES(3179 3180 3196 3197 3277 3278 3279 +
                          3476 3477 3486 3487 5251 5291 5292 5555 5962 +
                          6040 DHCF) PROMPT('Device type')
             PARM       KWD(MODEL) TYPE(*CHAR) LEN(4) RSTD(*YES) MIN(1) +
                          VALUES(0 1 2 4 5 11 12 23 24 25 26 27 A1 A2 +
                          B1 B2 C1 C2 D1 D2 E1 E2 F1 F2 G1 G2 H1 H2 +
                          M1 M2 W1 W2 *CALC) PROMPT('Device model')
             PARM       KWD(PORT) TYPE(*INT2) RANGE(0 17) PMTCTL(PL) +
                          PROMPT('Port number')
             PARM       KWD(SWTSET) TYPE(*INT2) RANGE(0 6) PMTCTL(PL) +
                          PROMPT('Switch setting')
             PARM       KWD(SHRSSNNBR) TYPE(*INT2) RANGE(0 3) +
                          PMTCTL(PL) PROMPT('Shared session number')
             PARM       KWD(LOCADR) TYPE(*CHAR) LEN(2) PMTCTL(PR) +
                          PROMPT('Local location address')
             PARM       KWD(RMTLOCNAME) TYPE(*NAME) LEN(8) PMTCTL(PR) +
                          PROMPT('Remote location')
             PARM       KWD(ONLINE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Online at IPL')
             PARM       KWD(CTL) TYPE(*NAME) LEN(10) +
                          PROMPT('Attached controller')
             PARM       KWD(KBDTYPE) TYPE(*CHAR) LEN(7) DFT(*SYSVAL) +
                          SPCVAL((*SYSVAL)) PMTCTL(*PMTRQS) +
                          PROMPT('Keyboard language type')
             PARM       KWD(DROP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Drop line at signoff')
             PARM       KWD(ALWBLN) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Allow blinking cursor')
             PARM       KWD(PRTDEV) TYPE(*NAME) LEN(10) DFT(*SYSVAL) +
                          SPCVAL((*SYSVAL)) PMTCTL(*PMTRQS) +
                          PROMPT('Print device')
             PARM       KWD(OUTQ) TYPE(Q1) DFT(*DEV) SNGVAL((*DEV)) +
                          PMTCTL(*PMTRQS) PROMPT('Output queue')
             PARM       KWD(PRTFILE) TYPE(Q2) PMTCTL(*PMTRQS) +
                          PROMPT('Printer file')
             PARM       KWD(WSCST) TYPE(Q1) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Workstation customizing')
             PARM       KWD(MSGQ) TYPE(Q3) PMTCTL(*PMTRQS) +
                          PROMPT('Message queue')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) +
                          PROMPT('Public authority')
             PL:        PMTCTL     CTL(DEVCLS) COND((*EQ *LCL))
             PR:        PMTCTL     CTL(DEVCLS) COND((*EQ *RMT))
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) DFT(QSYSPRT)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10) DFT(QSYSOPR)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
