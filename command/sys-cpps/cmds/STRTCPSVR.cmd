/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRTCPSVR: start TCP/IP servers through the servers framework       */
/* (net/servers): their jobs start with IBM's names in their           */
/* subsystems. HTTPSVR names the HTTP instance; INSTANCE the others'.  */
/* RESTART, HTTPSVR and INSTANCE are prompted as IBM's are.            */
             CMD        PROMPT('Start TCP/IP Server') MAXPOS(1) +
                          ALLOW(*ALL) MSGF(QTCPMSG) HLPPNLGRP(QTOCMD) +
                          HLPID(STRTCPSVR)
             PARM       KWD(SERVER) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*AUTOSTART) (*TELNET) +
                          (*FTP) (*SMTP) (*LPD) (*HTTP) (*SNMP) +
                          (*ROUTED) (*BOOTP) (*TFTP) (*REXEC) (*DHCP) +
                          (*DNS) (*POP) (*DDM) (*NTP) (*SSHD) +
                          (*HOSTSVR) (*QAP) (*NETSVR) (*DIRSRV)) +
                          MAX(300) CHOICE('*ALL, *AUTOSTART...') +
                          PROMPT('Server application')
             PARM       KWD(RESTART) TYPE(*CHAR) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE) (*HTTP) (*DNS)) +
                          PMTCTL(PRESTART) PROMPT('Restart server')
             PARM       KWD(HTTPSVR) TYPE(E1) DFT(*ALL) +
                          SNGVAL((*ALL)) PMTCTL(PHTTP) +
                          PROMPT('HTTP server')
             PARM       KWD(INSTANCE) TYPE(E2) DFT(*DFT) +
                          SNGVAL((*DFT) (*ALL) (*AUTOSTART)) +
                          PMTCTL(PINST) +
                          CHOICE('*DFT, *ALL, *AUTOSTART') +
                          PROMPT('Instance')
             E1:        ELEM       TYPE(*CHAR) LEN(32) SPCVAL((*ADMIN)) +
                          PROMPT('HTTP server instance')
                        ELEM       TYPE(*CHAR) LEN(256) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Instance startup values')
             E2:        ELEM       TYPE(*CHAR) LEN(32) +
                          CHOICE('Character value') +
                          PROMPT('Server instance')
                        ELEM       TYPE(*CHAR) LEN(300) DFT(*NONE) +
                          SPCVAL((*NONE)) CHOICE('Character value') +
                          PROMPT('Instance startup values')
 PRESTART:   PMTCTL     CTL(SERVER) COND((*EQ *HTTP) (*EQ *DNS) +
                          (*EQ *QOS)) NBRTRUE(*EQ 1)
 PHTTP:      PMTCTL     CTL(SERVER) COND((*EQ *HTTP)) NBRTRUE(*EQ 1)
 PINST:      PMTCTL     CTL(SERVER) COND((*NE *ALL) (*NE *AUTOSTART)) +
                          NBRTRUE(*EQ 2)
             DEP        CTL(&INSTANCE) PARM((&SERVER *EQ *ALL) +
                          (&SERVER *EQ *AUTOSTART)) NBRTRUE(*EQ 0) +
                          MSGID(TCP1617)
