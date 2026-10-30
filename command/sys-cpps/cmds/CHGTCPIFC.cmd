/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGTCPIFC: change an interface. */
             CMD        PROMPT('Change TCP/IP Interface') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) +
                          HLPID(CHGTCPIFC)
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(45) +
                          SPCVAL((*IP4DHCP) (*IP6SAC)) +
                          PROMPT('Internet address')
             PARM       KWD(LIND) TYPE(E1) DFT(*SAME) SNGVAL((*SAME) +
                          (*LOOPBACK) (*VIRTUALIP) (*OPC)) +
                          PROMPT('Line')
             PARM       KWD(SUBNETMASK) TYPE(*CHAR) LEN(15) DFT(*SAME) +
                          SPCVAL((*SAME) (*HOST '255.255.255.255')) +
                          PROMPT('Subnet mask')
             PARM       KWD(TOS) TYPE(*INT2) RSTD(*YES) DFT(*SAME) +
                          SPCVAL((*SAME 0) (*MINDELAY 2) (*MAXTHRPUT 3) +
                          (*MAXRLB 4) (*MINCOST 5) (*NORMAL 6)) +
                          PROMPT('Type of service')
             PARM       KWD(MTU) TYPE(*INT4) DFT(*SAME) RANGE(576 +
                          16388) SPCVAL((*SAME -1) (*LIND 0)) +
                          PROMPT('Largest datagram (MTU)')
             PARM       KWD(AUTOSTART) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME S) (*YES Y) (*NO N)) +
                          PROMPT('Start with TCP/IP')
             PARM       KWD(LCLIFC) TYPE(*CHAR) LEN(15) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) +
                          PROMPT('Associated local interface')
             PARM       KWD(ALIASNAME) TYPE(*CHAR) LEN(25) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PROMPT('Alias name')
             PARM       KWD(ADRPFXLEN) TYPE(*INT4) DFT(*SAME) RANGE(1 +
                          128) SPCVAL((*SAME -1)) +
                          PROMPT('Address prefix length')
             PARM       KWD(IFCID) TYPE(*CHAR) LEN(16) DFT(*SAME) +
                          SPCVAL((*SAME FFFFFFFFFFFFFFFF) (*LIND +
                          0000000000000000) (*GEN FFFFFFFFFFFFFFFE)) +
                          PROMPT('Interface identifier')
             PARM       KWD(DADMAXTRN) TYPE(*INT4) DFT(*SAME) RANGE(0 +
                          10) SPCVAL((*SAME -1)) +
                          PROMPT('Duplicate address checks')
             PARM       KWD(PVYEXN) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME S) (*NO N) (*YES Y)) +
                          PROMPT('Privacy extensions')
             PARM       KWD(PREFIFC) TYPE(*CHAR) LEN(15) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE) (*AUTO) (*DFTROUTE)) +
                          MAX(10) PROMPT('Preferred interfaces')
             PARM       KWD(PREFLIND) TYPE(E2) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) MAX(10) +
                          PROMPT('Preferred lines')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*BLANK ' ') (*SAME)) PROMPT('Text')
             PARM       KWD(DHCPDYNDNS) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*NO) (*YES)) +
                          PROMPT('DHCP dynamic DNS updates')
             PARM       KWD(DHCPCLTID) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*ADPTADR) +
                          (*DHCPUNQID)) +
                          PROMPT('DHCP client identifier')
             PARM       KWD(IP6GENGLBA) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*YES) (*NO)) +
                          PROMPT('Generate global addresses')
             E1:        ELEM       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Line description')
                        ELEM       TYPE(*INT4) DFT(*NONE) RANGE(1 4094) +
                          SPCVAL((*NONE 0)) PROMPT('VLAN identifier')
             E2:        ELEM       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Line description')
                        ELEM       TYPE(*INT4) DFT(*NONE) RANGE(1 4094) +
                          SPCVAL((*NONE 0)) PROMPT('VLAN identifier')
