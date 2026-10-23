/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRTCPSVR: start a TCP/IP server. Phase 8 has the telnet server;    */
/* the others wait for Phase 10.                                        */
             CMD        PROMPT('Start TCP/IP Server') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QTOCMD) HLPID(STRTCPSVR)
             PARM       KWD(SERVER) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*TELNET) (*FTP) (*SMTP) +
                          (*SNMP) (*HTTP) (*LPD) (*DNS) (*SSHD)) +
                          MAX(10) PROMPT('Server application')
             PARM       KWD(RESTART) TYPE(*CHAR) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE) (*HTTP) (*DNS)) +
                          PROMPT('Restart server')
