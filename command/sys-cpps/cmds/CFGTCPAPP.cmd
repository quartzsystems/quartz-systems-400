/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CFGTCPAPP: the TCP/IP applications menu. */
             CMD        PROMPT('Configure TCP/IP Applications') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) +
                          HLPID(CFGTCPAPP)
             PARM       KWD(APP) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SELECT) SPCVAL((*SELECT) (*SNMP) +
                          (*ROUTED) (*ROUTING *ROUTIN) (*TFTP) (*BOOTP) +
                          (*DDM) (*DHCP) (*TELNET) (*FTP) (*SMTP) +
                          (*LPD) (*HTTP) (*POP) (*REXEC) (*DNS) (*NTP)) +
                          PROMPT('Application')
