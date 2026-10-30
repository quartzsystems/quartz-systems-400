/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGSMTPA: the SMTP attributes, kept in the TCP/IP server table in   */
/* QUSRSYS. There is no SMTP server here: every value is kept, and the */
/* outbound mail of SNDSMTPEMM and QtmmSendMail goes to FWDHUBSVR or   */
/* MAILROUTER. Keywords, types and special values are the oracle's     */
/* (V7R5M0); which parameters the prompt shows follows its captures.   */
             CMD        PROMPT('Change SMTP Attributes') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTMCMD) HLPID(CHGSMTPA)
             PARM       KWD(AUTOSTART) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Autostart server')
             PARM       KWD(COLDSTART) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Clear e-mail on start-up')
             PARM       KWD(DIRTYPE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SMTP *SMTPMSF *SDD) +
                          PROMPT('E-mail directory type')
             PARM       KWD(RTYMIN) TYPE(ER99) PROMPT('Retries by minute')
             PARM       KWD(RTYDAY) TYPE(ER9) PROMPT('Retries by day')
             PARM       KWD(RTYHOUR) TYPE(ER0) PROMPT('Retries by hour')
             PARM       KWD(RTYRMTSVR) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PMTCTL(PSDD) PROMPT('Retry remote server')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 65533) SPCVAL((*SAME -1) (*DFT 819)) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Coded character set identifier')
             PARM       KWD(JOURNAL) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PMTCTL(*PMTRQS) PROMPT('Journal')
             PARM       KWD(PCTRTGCHR) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PMTCTL(PSDD) PROMPT('Percent routing character')
             PARM       KWD(ETRNSVR) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Support ETRN for server')
             PARM       KWD(MIME8BIT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PMTCTL(PSDD) PROMPT('Support 8-bit MIME')
             PARM       KWD(NFYDLVRY) TYPE(EN) PMTCTL(*PMTRQS) +
                          PROMPT('Delivery status notification')
             PARM       KWD(SBSD) TYPE(QS) DFT(*SAME) +
                          SNGVAL((*SAME) (*DFT)) PMTCTL(*PMTRQS) +
                          PROMPT('Subsystem description')
             PARM       KWD(RBLSVR) TYPE(*CHAR) LEN(256) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Realtime Blackhole List')
             PARM       KWD(ALWRLY) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          MAX(3) DFT(*SAME) VALUES(*LIST *NEAR +
                          *POPWDW) SNGVAL((*SAME) (*NONE) (*ALL) +
                          (*BOTH)) PMTCTL(*PMTRQS) +
                          PROMPT('Allow relayed mail')
             PARM       KWD(POPWDW) TYPE(*INT4) DFT(*SAME) +
                          RANGE(15 65535) SPCVAL((*SAME -1) (*NONE 0)) +
                          PMTCTL(*PMTRQS) PROMPT('POP send mail window')
             PARM       KWD(IFCDMN) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *LIST) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Interface/domain association')
             PARM       KWD(FTRACN) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *KEEP *DISCARD) +
                          PMTCTL(*PMTRQS) PROMPT('Filter mail for virus')
             PARM       KWD(OVRRJTCNNL) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Override reject connect list')
             PARM       KWD(ALWBARELF) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Allow bare line feed')
             PARM       KWD(VFYID) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Verify identification')
             PARM       KWD(ALWAUTH) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *RELAY *LCLRLY +
                          *ANONYM *NONE) PMTCTL(*PMTRQS) +
                          PROMPT('Allow authentication')
             PARM       KWD(VFYMSFMSG) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PMTCTL(*PMTRQS) PROMPT('Verify MSF messages')
             PARM       KWD(VFYFROMUSR) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ALL *LIST *NONE) +
                          PMTCTL(*PMTRQS) PROMPT('Verify from user')
             PARM       KWD(FWDHUBSVR) TYPE(*CHAR) LEN(257) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Forwarding mailhub server')
             PARM       KWD(AUTOADD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PMTCTL(PSDD) PROMPT('Automatic registration')
             PARM       KWD(USRIDPFX) TYPE(*CNAME) LEN(3) DFT(*SAME) +
                          SPCVAL((*SAME) (*DFT QSM)) PMTCTL(PSDD) +
                          PROMPT('User ID prefix')
             PARM       KWD(ADDRESS) TYPE(*CNAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME) (*DFT QSMRMTAD)) PMTCTL(PSDD) +
                          PROMPT('Address')
             PARM       KWD(SYSNAME) TYPE(*CHAR) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME) (*DFT TCPIP)) PMTCTL(PSDD) +
                          PROMPT('System name')
             PARM       KWD(TBLTYPE) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSTEM *PERSONAL) +
                          PMTCTL(PSDD) PROMPT('Alias table type')
             PARM       KWD(USRIDDELIM) TYPE(*CHAR) LEN(1) +
                          DFT(*SAME) SPCVAL((*SAME) (*DFT)) +
                          PMTCTL(PSDD) PROMPT('User ID delimiter')
             PARM       KWD(MAILROUTER) TYPE(*CHAR) LEN(256) +
                          DFT(*SAME) SPCVAL((*SAME) (*NONE)) +
                          PMTCTL(PSDD) PROMPT('Mail router')
             PARM       KWD(TBLSMTPOUT) TYPE(QT) DFT(*SAME) +
                          SNGVAL((*SAME) (*CCSID) (*DFT)) PMTCTL(PSDD) +
                          PROMPT('Outgoing EBCDIC/ASCII table')
             PARM       KWD(TBLSMTPIN) TYPE(QT) DFT(*SAME) +
                          SNGVAL((*SAME) (*CCSID) (*DFT)) PMTCTL(PSDD) +
                          PROMPT('Incoming ASCII/EBCDIC table')
             PARM       KWD(FIREWALL) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PMTCTL(PSDD) PROMPT('Firewall')
             PARM       KWD(ALLMAILMSF) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PMTCTL(PSDD) PROMPT('Journal all mail on MSF')
             PARM       KWD(DIALSCD) TYPE(ED) SNGVAL((*NO)) +
                          PMTCTL(PSDD) PROMPT('Dial-up scheduler')
             PARM       KWD(ETRNCLT) TYPE(EE) SNGVAL((*NO)) +
                          PMTCTL(PSDD) PROMPT('Support ETRN for client')
             PARM       KWD(ALWIMPSSL) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Allow e-mail over implicit SSL')
             PARM       KWD(MAXINEMM) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 1240000) SPCVAL((*SAME -1) (*DFT 0)) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Max incoming e-mail messages')
             PARM       KWD(MAXOUTEMM) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 1240000) SPCVAL((*SAME -1) (*DFT 0)) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Max outgoing e-mail messages')
             PARM       KWD(KEEPUNTIL) TYPE(EK) PMTCTL(*PMTRQS) +
                          PROMPT('Keep e-mail until')
             PARM       KWD(MAXEMMSIZE) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1000 2000000) SPCVAL((*SAME -1) (*DFT +
                          0)) PMTCTL(*PMTRQS) +
                          PROMPT('Maximum e-mail size')
             PARM       KWD(ALIASDMN) TYPE(*CHAR) LEN(385) MAX(10) +
                          DFT(*SAME) SNGVAL((*SAME) (*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Alias domain')
             PARM       KWD(OAUTH) TYPE(EO) PMTCTL(*PMTRQS) +
                          PROMPT('OAuth')
 ER99:       ELEM       TYPE(*INT2) DFT(*SAME) RANGE(0 99) +
                          SPCVAL((*SAME -1) (*DFT 3)) +
                          PROMPT('Number of retries')
             ELEM       TYPE(*INT2) DFT(*SAME) RANGE(0 99) +
                          SPCVAL((*SAME -1) (*DFT 30)) +
                          PROMPT('Time interval')
 ER9:        ELEM       TYPE(*INT2) DFT(*SAME) RANGE(0 9) +
                          SPCVAL((*SAME -1) (*DFT 0)) +
                          PROMPT('Number of retries')
             ELEM       TYPE(*INT2) DFT(*SAME) RANGE(0 9) +
                          SPCVAL((*SAME -1) (*DFT 0)) +
                          PROMPT('Time interval')
 ER0:        ELEM       TYPE(*INT2) DFT(*SAME) RANGE(0 99) +
                          SPCVAL((*SAME -1) (*DFT 0)) +
                          PROMPT('Number of retries')
             ELEM       TYPE(*INT2) DFT(*SAME) RANGE(0 99) +
                          SPCVAL((*SAME -1) (*DFT 0)) +
                          PROMPT('Time interval')
 EN:         ELEM       TYPE(*CHAR) LEN(256) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) +
                          PROMPT('Responsible person')
 QS:         QUAL       TYPE(*NAME) LEN(10) MIN(1)
             QUAL       TYPE(*NAME) LEN(10) MIN(1) PROMPT('Library')
 QT:         QUAL       TYPE(*NAME) LEN(10) MIN(1)
             QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
 ED:         ELEM       TYPE(*CHAR) LEN(5) RSTD(*YES) DFT(*SAME) +
                          SPCVAL((*SAME) (*YES)) PROMPT('Scheduler')
             ELEM       TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Connection profile')
             ELEM       TYPE(*INT4) DFT(*SAME) RANGE(1 1440) +
                          SPCVAL((*SAME -1) (*NONE 0)) +
                          PROMPT('Timer')
 EE:         ELEM       TYPE(*CHAR) LEN(5) RSTD(*YES) DFT(*SAME) +
                          SPCVAL((*SAME) (*YES)) PROMPT('Client')
             ELEM       TYPE(*CHAR) LEN(40) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Domain name')
             ELEM       TYPE(*CHAR) LEN(256) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Server')
 EK:         ELEM       TYPE(*INT4) DFT(*SAME) RANGE(0 1240000) +
                          SPCVAL((*SAME -1) (*DFT 0)) +
                          PROMPT('Incoming')
             ELEM       TYPE(*INT4) DFT(*SAME) RANGE(0 1240000) +
                          SPCVAL((*SAME -1) (*DFT 0)) +
                          PROMPT('Outgoing')
 EO:         ELEM       TYPE(*CHAR) LEN(255) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PROMPT('Client ID')
             ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) DFT(*SAME) +
                          VALUES(*SAME *ENABLE *DELETE) +
                          PROMPT('Action')
 PSDD:       PMTCTL     CTL(DIRTYPE) COND((*EQ *SDD))
