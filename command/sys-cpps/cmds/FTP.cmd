/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* FTP: the FTP client. Subcommands come from the display, or from the */
/* file INPUT when it is overridden (batch); what the session prints   */
/* goes to the display, or to the file OUTPUT when it is overridden.   */
/* Keywords, ranges and defaults are the oracle's (V7R5M0).             */
             CMD        PROMPT('Start TCP/IP File Transfer') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTMCMD) HLPID(STRTCPFTP)
             PARM       KWD(RMTSYS) TYPE(*CHAR) LEN(255) MIN(1) +
                          SPCVAL((*INTNETADR)) PROMPT('Remote system')
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(64) +
                          PMTCTL(PC1) PROMPT('Internet address')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*DFT) +
                          RANGE(1 65533) SPCVAL((*DFT 819)) +
                          PROMPT('Coded character set identifier')
             PARM       KWD(TBLFTPOUT) TYPE(Q1) DFT(*CCSID) +
                          SNGVAL((*CCSID) (*DFT)) PMTCTL(*PMTRQS) +
                          PROMPT('Outgoing EBCDIC/ASCII table')
             PARM       KWD(TBLFTPIN) TYPE(Q1) DFT(*CCSID) +
                          SNGVAL((*CCSID) (*DFT)) PMTCTL(*PMTRQS) +
                          PROMPT('Incoming ASCII/EBCDIC table')
             PARM       KWD(PORT) TYPE(*INT4) DFT(*DFT) +
                          RANGE(1 65535) SPCVAL((*DFT 21) (*SECURE +
                          990)) PROMPT('Port')
             PARM       KWD(SECCNN) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*DFT) VALUES(*DFT *NONE *SSL *IMPLICIT +
                          *KERBEROS) PROMPT('Secure connection')
             PARM       KWD(DTAPROT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*DFT) VALUES(*DFT *CLEAR *SAFE *PRIVATE) +
                          PROMPT('Data protection')
             PARM       KWD(APPID) TYPE(*CHAR) LEN(100) DFT(*DFT) +
                          SPCVAL((*DFT QIBM_QTMF_FTP_CLIENT)) +
                          PMTCTL(*PMTRQS) PROMPT('Application identifier')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             PC1:       PMTCTL     CTL(RMTSYS) COND((*EQ *INTNETADR))
             DEP        CTL((&RMTSYS *EQ *INTNETADR)) +
                          PARM(&INTNETADR) MSGID(CPD0150)
