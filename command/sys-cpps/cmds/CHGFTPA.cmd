/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGFTPA: the FTP server's attributes, kept in the TCP/IP server     */
/* table in QUSRSYS and read when the server and its sessions start.   */
/* Keywords, ranges and defaults are the oracle's (V7R5M0).             */
             CMD        PROMPT('Change FTP Attributes') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTMCMD) HLPID(CHGFTPA)
             PARM       KWD(AUTOSTART) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Autostart servers')
             PARM       KWD(NBRSVR) TYPE(*INT2) DFT(*SAME) +
                          RANGE(1 20) SPCVAL((*SAME -1) (*DFT 3)) +
                          PROMPT('Number of initial servers')
             PARM       KWD(INACTTIMO) TYPE(*INT4) DFT(*SAME) +
                          RANGE(0 2147483) SPCVAL((*SAME -1) (*DFT +
                          300)) PROMPT('Inactivity timeout')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 65533) SPCVAL((*SAME -1) (*DFT 819)) +
                          PROMPT('Coded character set identifier')
             PARM       KWD(TBLFTPOUT) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*CCSID) (*DFT)) +
                          PROMPT('Outgoing EBCDIC/ASCII table')
             PARM       KWD(TBLFTPIN) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*CCSID) (*DFT)) +
                          PROMPT('Incoming ASCII/EBCDIC table')
             PARM       KWD(NAMEFMT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *LIB *PATH) +
                          PROMPT('Initial name format')
             PARM       KWD(CURDIR) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *CURLIB *HOMEDIR) +
                          PROMPT('Initial directory')
             PARM       KWD(LISTFMT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *DFT *UNIX) +
                          PROMPT('Initial list format')
             PARM       KWD(CRTCCSID) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 65533) SPCVAL((*SAME -1) (*CALC 0) +
                          (*USER -3) (*SYSVAL -2)) +
                          PROMPT('Create database files CCSID')
             PARM       KWD(SBSD) TYPE(Q2) DFT(*SAME) +
                          SNGVAL((*SAME) (*DFT)) +
                          PROMPT('Subsystem description')
             PARM       KWD(ALWSSL) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO *ONLY) +
                          PROMPT('Allow secure sockets layer')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Library')
