/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRTCPSVR: start TCP/IP servers through the servers framework       */
/* (net/servers): their jobs start with IBM's names in their           */
/* subsystems. HTTPSVR names the HTTP instance; INSTANCE the others'.  */
             CMD        PROMPT('Start TCP/IP Server') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QTOCMD) HLPID(STRTCPSVR)
             PARM       KWD(SERVER) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*AUTOSTART) (*TELNET) +
                          (*FTP) (*SMTP) (*LPD) (*HTTP) (*SNMP) +
                          (*ROUTED) (*BOOTP) (*TFTP) (*REXEC) (*DHCP) +
                          (*DNS) (*POP) (*DDM) (*NTP) (*SSHD) +
                          (*HOSTSVR)) MAX(300) +
                          PROMPT('Server application')
             PARM       KWD(RESTART) TYPE(*CHAR) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE) (*HTTP) (*DNS)) +
                          PROMPT('Restart server')
             PARM       KWD(HTTPSVR) TYPE(E1) DFT(*ALL) +
                          SNGVAL((*ALL)) PROMPT('HTTP server')
             PARM       KWD(INSTANCE) TYPE(*CHAR) LEN(32) DFT(*DFT) +
                          SPCVAL((*DFT) (*ALL) (*AUTOSTART)) +
                          PROMPT('Server instance')
             E1:        ELEM       TYPE(*CHAR) LEN(32) SPCVAL((*ADMIN)) +
                          PROMPT('HTTP server instance')
                        ELEM       TYPE(*CHAR) LEN(256) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Instance startup values')
