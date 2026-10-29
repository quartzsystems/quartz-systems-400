/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGDEVPRT: change a printer device description; what is not given */
/* stays as it is.                                                    */
             CMD        PROMPT('Change Device Desc (Printer)') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHDCDEV) +
                          HLPID(CHGDEVPRT)
             PARM       KWD(DEVD) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Device description')
             PARM       KWD(PORT) TYPE(*INT4) DFT(*SAME) RANGE(0 65535) +
                          SPCVAL((*SAME -1)) PROMPT('Port number')
             PARM       KWD(ONLINE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Online at IPL')
             PARM       KWD(PRTQLTY) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *STD *DRAFT *NLQ) +
                          PROMPT('Print quality')
             PARM       KWD(FONT) TYPE(E1) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Font')
             PARM       KWD(FORMFEED) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *TYPE *CONT *CONT2 +
                          *CUT *AUTOCUT) PROMPT('Form feed')
             PARM       KWD(SEPDRAWER) TYPE(*INT2) DFT(*SAME) +
                          RANGE(1 255) SPCVAL((*SAME -1) (*FILE 0)) +
                          PROMPT('Separator drawer')
             PARM       KWD(SEPPGM) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) +
                          PROMPT('Separator program')
             PARM       KWD(PRTERRMSG) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *INQ *INFO) +
                          PROMPT('Printer error message')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*CTLD) (*SYSOPR)) +
                          PROMPT('Message queue')
             PARM       KWD(TRANSFORM) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Host print transform')
             PARM       KWD(MFRTYPMDL) TYPE(*CHAR) LEN(15) DFT(*SAME) +
                          SPCVAL((*SAME) (*WSCST) (*IBM42011) (*HP4) +
                          (*HP5) (*HP5SI) (*HPII) (*HPIID) (*HPIIP) +
                          (*HPIII) (*HPIIID) (*HPIIIP) (*HPIIISI) +
                          (*HPPLUS) (*HPLJ4) (*IBM3812) (*IBM4019)) +
                          PROMPT('Manufacturer type and model')
             PARM       KWD(PPRSRC1) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *MFRTYPMDL *LETTER +
                          *LEGAL *EXECUTIVE *LEDGER *A3 *A4 *A5 *B4 *B5 +
                          *CONT80 *CONT132) PROMPT('Paper source 1')
             PARM       KWD(PPRSRC2) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *MFRTYPMDL *LETTER +
                          *LEGAL *EXECUTIVE *LEDGER *A3 *A4 *A5 *B4 *B5 +
                          *CONT80 *CONT132) PROMPT('Paper source 2')
             PARM       KWD(ENVELOPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *MFRTYPMDL *MONARCH +
                          *NUMBER9 *NUMBER10 *B5 *C5 *DL *NONE) +
                          PROMPT('Envelope source')
             PARM       KWD(MAXPNDRQS) TYPE(*INT2) DFT(*SAME) +
                          RANGE(1 31) SPCVAL((*SAME -1)) +
                          PROMPT('Maximum pending requests')
             PARM       KWD(PRTRQSTMR) TYPE(*INT2) DFT(*SAME) +
                          RANGE(1 3600) SPCVAL((*SAME -1) (*NOMAX 0)) +
                          PROMPT('Print request timer')
             PARM       KWD(CHRID) TYPE(E2) DFT(*SAME) +
                          SNGVAL((*SAME) (*SYSVAL)) +
                          PROMPT('Character identifier')
             PARM       KWD(RMTLOCNAME) TYPE(*CHAR) LEN(255) +
                          DFT(*SAME) SPCVAL((*SAME)) CASE(*MIXED) +
                          PROMPT('Remote location')
             PARM       KWD(WSCST) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) +
                          PROMPT('Workstation customizing object')
             PARM       KWD(USRDTATFM) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) +
                          PROMPT('User data transform')
             PARM       KWD(USRDRVPGM) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) +
                          PROMPT('User driver program')
             PARM       KWD(SYSDRVPGM) TYPE(*CHAR) LEN(15) DFT(*SAME) +
                          SPCVAL((*SAME) (*HPPJLDRV) (*IBMPJLDRV) +
                          (*NETSTNDRV) (*IBMSNMPDRV) (*IBMIPPDRV)) +
                          PROMPT('System driver program')
             PARM       KWD(SECURECNN) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Secure connection')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Text ''description''')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(10) PROMPT('Identifier')
                        ELEM       TYPE(*DEC) LEN(4 1) DFT(*NONE) +
                          SPCVAL((*NONE 0)) PROMPT('Point size')
             E2:        ELEM       TYPE(*INT2) RANGE(1 32767) MIN(1) +
                          PROMPT('Graphic character set')
                        ELEM       TYPE(*INT2) RANGE(1 32767) MIN(1) +
                          PROMPT('Code page')
