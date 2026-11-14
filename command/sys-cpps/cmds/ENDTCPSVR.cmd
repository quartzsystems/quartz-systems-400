/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDTCPSVR: end TCP/IP servers through the servers framework.        */
/* HTTPSVR and INSTANCE are prompted as IBM's are; OPTION and DELAY,   */
/* QS/400's, with HTTPSVR for the HTTP server's instances.             */
             CMD        PROMPT('End TCP/IP Server') MAXPOS(1) +
                          ALLOW(*ALL) MSGF(QTCPMSG) HLPPNLGRP(QTOCMD) +
                          HLPID(ENDTCPSVR)
             PARM       KWD(SERVER) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*TELNET) (*FTP) (*SMTP) +
                          (*LPD) (*HTTP) (*SNMP) (*ROUTED) (*BOOTP) +
                          (*TFTP) (*REXEC) (*DHCP) (*DNS) (*POP) +
                          (*DDM) (*NTP) (*SSHD) (*HOSTSVR) (*QAP) (*NETSVR) +
                          (*DIRSRV)) MAX(300) CHOICE('*ALL...') +
                          PROMPT('Server application')
             PARM       KWD(HTTPSVR) TYPE(*CHAR) LEN(32) DFT(*ALL) +
                          SPCVAL((*ALL) (*ADMIN)) PMTCTL(PHTTP) +
                          PROMPT('HTTP server')
             PARM       KWD(INSTANCE) TYPE(*CHAR) LEN(32) DFT(*DFT) +
                          SPCVAL((*DFT) (*ALL)) PMTCTL(PINST) +
                          CHOICE('Character value') PROMPT('Instance')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*CNTRLD) VALUES(*CNTRLD *IMMED) +
                          PMTCTL(PHTTP) PROMPT('How to end')
             PARM       KWD(DELAY) TYPE(*INT4) DFT(30) RANGE(1 86400) +
                          PMTCTL(PHTTP) +
                          PROMPT('Delay for *CNTRLD (seconds)')
 PHTTP:      PMTCTL     CTL(SERVER) COND((*EQ *HTTP)) NBRTRUE(*EQ 1)
 PINST:      PMTCTL     CTL(SERVER) COND((*NE *ALL)) NBRTRUE(*EQ 1)
             DEP        CTL(&SERVER *EQ *ALL) PARM((&INSTANCE *NE *DFT)) +
                          NBRTRUE(*EQ 0) MSGID(TCP1617)
