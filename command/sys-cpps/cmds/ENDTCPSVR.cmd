/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDTCPSVR: end TCP/IP servers through the servers framework.        */
             CMD        PROMPT('End TCP/IP Server') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QTOCMD) HLPID(ENDTCPSVR)
             PARM       KWD(SERVER) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*TELNET) (*FTP) (*SMTP) +
                          (*LPD) (*HTTP) (*SNMP) (*ROUTED) (*BOOTP) +
                          (*TFTP) (*REXEC) (*DHCP) (*DNS) (*POP) +
                          (*DDM) (*NTP) (*SSHD) (*HOSTSVR)) MAX(300) +
                          PROMPT('Server application')
             PARM       KWD(HTTPSVR) TYPE(*CHAR) LEN(32) DFT(*ALL) +
                          SPCVAL((*ALL) (*ADMIN)) PROMPT('HTTP server')
             PARM       KWD(INSTANCE) TYPE(*CHAR) LEN(32) DFT(*DFT) +
                          SPCVAL((*DFT) (*ALL)) PROMPT('Server instance')
