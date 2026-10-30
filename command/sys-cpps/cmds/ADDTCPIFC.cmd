/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDTCPIFC: add an interface: an address on a line. */
             CMD        PROMPT('Add TCP/IP Interface') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) +
                          HLPID(ADDTCPIFC)
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(45) +
                          SPCVAL((*IP4DHCP) (*IP6SAC)) MIN(1) +
                          PROMPT('Internet address')
             PARM       KWD(LIND) TYPE(E1) SNGVAL((*LOOPBACK) +
                          (*VIRTUALIP) (*OPC)) MIN(1) PROMPT('Line')
             PARM       KWD(SUBNETMASK) TYPE(*CHAR) LEN(15) +
                          SPCVAL((*HOST '255.255.255.255')) +
                          PROMPT('Subnet mask')
             PARM       KWD(TOS) TYPE(*INT2) RSTD(*YES) DFT(*NORMAL) +
                          SPCVAL((*MINDELAY 2) (*MAXTHRPUT 3) (*MAXRLB +
                          4) (*MINCOST 5) (*NORMAL 6)) +
                          PROMPT('Type of service')
             PARM       KWD(MTU) TYPE(*INT4) DFT(*LIND) RANGE(576 +
                          16388) SPCVAL((*LIND 0)) +
                          PROMPT('Largest datagram (MTU)')
             PARM       KWD(AUTOSTART) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*YES) SPCVAL((*YES Y) (*NO N)) +
                          PROMPT('Start with TCP/IP')
             PARM       KWD(LCLIFC) TYPE(*CHAR) LEN(15) DFT(*NONE) +
                          SPCVAL((*NONE)) +
                          PROMPT('Associated local interface')
             PARM       KWD(ALIASNAME) TYPE(*CHAR) LEN(25) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Alias name')
             PARM       KWD(ADRPFXLEN) TYPE(*INT4) DFT(64) RANGE(1 128) +
                          PROMPT('Address prefix length')
             PARM       KWD(IFCID) TYPE(*CHAR) LEN(16) DFT(*LIND) +
                          SPCVAL((*LIND 0000000000000000) (*GEN +
                          FFFFFFFFFFFFFFFE)) +
                          PROMPT('Interface identifier')
             PARM       KWD(DADMAXTRN) TYPE(*INT4) DFT(1) RANGE(0 10) +
                          PROMPT('Duplicate address checks')
             PARM       KWD(PVYEXN) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*NO) SPCVAL((*NO N) (*YES Y)) +
                          PROMPT('Privacy extensions')
             PARM       KWD(PREFIFC) TYPE(*CHAR) LEN(15) DFT(*NONE) +
                          SNGVAL((*NONE) (*AUTO) (*DFTROUTE)) MAX(10) +
                          PROMPT('Preferred interfaces')
             PARM       KWD(PREFLIND) TYPE(E2) DFT(*NONE) +
                          SNGVAL((*NONE)) MAX(10) +
                          PROMPT('Preferred lines')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Text')
             PARM       KWD(DHCPDYNDNS) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NO) SPCVAL((*NO) (*YES)) +
                          PROMPT('DHCP dynamic DNS updates')
             PARM       KWD(DHCPCLTID) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ADPTADR) SPCVAL((*ADPTADR) (*DHCPUNQID)) +
                          PROMPT('DHCP client identifier')
             PARM       KWD(IP6GENGLBA) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*YES) SPCVAL((*YES) (*NO)) +
                          PROMPT('Generate global addresses')
             E1:        ELEM       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Line description')
                        ELEM       TYPE(*INT4) DFT(*NONE) RANGE(1 4094) +
                          SPCVAL((*NONE 0)) PROMPT('VLAN identifier')
             E2:        ELEM       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Line description')
                        ELEM       TYPE(*INT4) DFT(*NONE) RANGE(1 4094) +
                          SPCVAL((*NONE 0)) PROMPT('VLAN identifier')
