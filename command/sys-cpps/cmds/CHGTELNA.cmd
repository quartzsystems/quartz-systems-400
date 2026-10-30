/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGTELNA: the telnet server's attributes, kept in the TCP/IP server */
/* table in QUSRSYS and read when the server starts. Keywords, ranges  */
/* and defaults are the oracle's (V7R5M0).                              */
             CMD        PROMPT('Change TELNET Attributes') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTGCMD) HLPID(CHGTELNA)
             PARM       KWD(AUTOSTART) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*YES *NO *SAME) +
                          PROMPT('Autostart server')
             PARM       KWD(NBRSVR) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 200) SPCVAL((*SAME -1) (*CALC -2)) +
                          PROMPT('Number servers')
             PARM       KWD(NBRTASK) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 200) SPCVAL((*SAME -1) (*CALC -2)) +
                          PROMPT('Number of tasks')
             PARM       KWD(TIMMRKTIMO) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 2419200) SPCVAL((*SAME -1) (*CALC +
                          -2) (*DFT -2)) PROMPT('Session keep alive +
                          timeout')
             PARM       KWD(ENBWIDE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*YES *NO *SAME) +
                          PROMPT('Enable 3270 wide-screen')
             PARM       KWD(NBRFLDS) TYPE(*INT4) DFT(*DFT) +
                          RANGE(126 9999) SPCVAL((*DFT -2) (*RPY -3)) +
                          PROMPT('Number of 3270 input fields')
             PARM       KWD(DFTNVTTYPE) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*VT100) (*NVT)) +
                          PROMPT('Default NVT type')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 65533) SPCVAL((*SAME -1) (*MULTINAT +
                          84542540) (*BRITISH 84608077) (*DUTCH +
                          84673614) (*FINNISH 84739151) (*FRENCH +
                          18941008) (*FRENCHCAN 78119932) (*GERMAN +
                          17368051) (*ITALIAN 19203060) (*NORDAN +
                          84870225) (*SPANISH 78316543) (*SWEDISH +
                          84935762) (*SWISS 78185469)) +
                          PROMPT('Coded character set identifier')
             PARM       KWD(TBLVTOUT) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*CCSID) (*DFT)) +
                          PROMPT('Outgoing EBCDIC/ASCII table')
             PARM       KWD(TBLVTIN) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*CCSID) (*DFT)) +
                          PROMPT('Incoming ASCII/EBCDIC table')
             PARM       KWD(ALWSSL) TYPE(*NAME) LEN(5) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*YES) (*NO) (*ONLY) +
                          (*SAME)) PROMPT('Allow Transport Layer +
                          Security')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
