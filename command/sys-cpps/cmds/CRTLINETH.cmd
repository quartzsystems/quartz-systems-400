/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTLINETH: describe an Ethernet line over a port, an aggregate of */
/* ports or a network server port.                                    */
             CMD        PROMPT('Create Line Desc (Ethernet)') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDCLIN) HLPID(CRTLINETH)
             PARM       KWD(LIND) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Line description')
             PARM       KWD(RSRCNAME) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*AGG) (*NWSD)) PROMPT('Resource name')
             PARM       KWD(BRIDGE) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Bridge identifier')
             PARM       KWD(ONLINE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Online at IPL')
             PARM       KWD(VRYWAIT) TYPE(*INT2) DFT(*NOWAIT) +
                          RANGE(15 180) SPCVAL((*NOWAIT 0)) +
                          PROMPT('Vary on wait')
             PARM       KWD(AGGPCY) TYPE(E1) PMTCTL(PAGG) +
                          PROMPT('Aggregate policy')
             PARM       KWD(AGGRSCL) TYPE(*NAME) LEN(10) MAX(8) +
                          PMTCTL(PAGG) PROMPT('Aggregated resource list')
             PARM       KWD(NETCTL) TYPE(*NAME) LEN(10) PMTCTL(*PMTRQS) +
                          PROMPT('Network controller')
             PARM       KWD(NWS) TYPE(E2) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(PNWS) PROMPT('Network server description')
             PARM       KWD(ASSOCPORT) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(PNWS) +
                          PROMPT('Associated port resource name')
             PARM       KWD(ADPTADR) TYPE(*CHAR) LEN(12) DFT(*ADPT) +
                          SPCVAL((*ADPT)) PROMPT('Local adapter address')
             PARM       KWD(GRPADR) TYPE(*CHAR) LEN(12) MAX(12) +
                          DFT(*NONE) SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Group address')
             PARM       KWD(EXCHID) TYPE(*CHAR) LEN(8) DFT(*SYSGEN) +
                          SPCVAL((*SYSGEN)) PMTCTL(*PMTRQS) +
                          PROMPT('Exchange identifier')
             PARM       KWD(ETHSTD) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*ETHV2) VALUES(*ETHV2 *IEEE8023 *ALL) +
                          PROMPT('Ethernet standard')
             PARM       KWD(LINESPEED) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*AUTO) VALUES(10M 100M 1G 10G 25G 100G +
                          *AUTO) PROMPT('Line speed')
             PARM       KWD(DUPLEX) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*AUTO) VALUES(*HALF *FULL *AUTO) +
                          PROMPT('Duplex')
             PARM       KWD(SRVOPT) TYPE(*CHAR) LEN(5) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Serviceability options')
             PARM       KWD(MAXFRAME) TYPE(*INT2) DFT(1496) +
                          RANGE(1496 8996) PROMPT('Maximum frame size')
             PARM       KWD(SSAP) TYPE(E3) MAX(24) DFT(*SYSGEN) +
                          SNGVAL((*SYSGEN)) PMTCTL(*PMTRQS) +
                          PROMPT('SSAP list')
             PARM       KWD(MAXCTL) TYPE(*INT2) DFT(40) RANGE(1 256) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum controllers')
             PARM       KWD(COSTCNN) TYPE(*INT2) DFT(0) RANGE(0 255) +
                          PMTCTL(*PMTRQS) PROMPT('Cost/connect time')
             PARM       KWD(COSTBYTE) TYPE(*INT2) DFT(0) RANGE(0 255) +
                          PMTCTL(*PMTRQS) PROMPT('Cost/byte')
             PARM       KWD(SECURITY) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NONSECURE) VALUES(*NONSECURE *PKTSWTNET +
                          *UNDGRDCBL *SECURECND *GUARDCND *ENCRYPTED +
                          *MAX) PMTCTL(*PMTRQS) PROMPT('Security for line')
             PARM       KWD(PRPDLY) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*LAN) VALUES(*MIN *LAN *TELEPHONE +
                          *PKTSWTNET *SATELLITE *MAX) PMTCTL(*PMTRQS) +
                          PROMPT('Propagation delay')
             PARM       KWD(USRDFN1) TYPE(*INT2) DFT(128) RANGE(0 255) +
                          PMTCTL(*PMTRQS) PROMPT('User-defined 1')
             PARM       KWD(USRDFN2) TYPE(*INT2) DFT(128) RANGE(0 255) +
                          PMTCTL(*PMTRQS) PROMPT('User-defined 2')
             PARM       KWD(USRDFN3) TYPE(*INT2) DFT(128) RANGE(0 255) +
                          PMTCTL(*PMTRQS) PROMPT('User-defined 3')
             PARM       KWD(AUTOCRTCTL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Autocreate controller')
             PARM       KWD(AUTODLTCTL) TYPE(*INT4) DFT(1440) +
                          RANGE(1 10000) SPCVAL((*NONE 0)) +
                          PMTCTL(*PMTRQS) PROMPT('Autodelete controller')
             PARM       KWD(CMNRCYLMT) TYPE(E4) SNGVAL((*SYSVAL -1)) +
                          DFT(*SYSVAL) PMTCTL(*PMTRQS) +
                          PROMPT('Recovery limits')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*SYSVAL) SNGVAL((*SYSVAL) +
                          (*SYSOPR)) PMTCTL(*PMTRQS) PROMPT('Message queue')
             PARM       KWD(LINKSPEED) TYPE(*CHAR) LEN(8) DFT(*MAX) +
                          SPCVAL((*MIN) (*MAX)) PMTCTL(*PMTRQS) +
                          PROMPT('Link speed')
             PARM       KWD(THRESHOLD) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*OFF) VALUES(*OFF *MIN *MED *MAX) +
                          PMTCTL(*PMTRQS) PROMPT('Error threshold level')
             PARM       KWD(GENTSTFRM) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Generate test frame')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*CHANGE) +
                          SPCVAL((*CHANGE) (*ALL) (*USE) (*EXCLUDE) +
                          (*LIBCRTAUT)) PMTCTL(*PMTRQS) +
                          PROMPT('Authority')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK '')) PROMPT('Text ''description''')
 E1:         ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) MIN(1) +
                          VALUES(*ETHCHL *LNKAGG) PROMPT('Standard')
             ELEM       TYPE(*CHAR) LEN(9) RSTD(*YES) MIN(1) +
                          VALUES(*DFT *SRCPORT *DESTPORT *SRCDESTP +
                          *RNDRBN) PROMPT('Policy type')
 E2:         ELEM       TYPE(*NAME) LEN(8)
             ELEM       TYPE(*INT2) RANGE(1 2) SPCVAL((*VRTETHPTP 15) +
                          (*VRTETH0 14) (*VRTETH1 13) (*VRTETH2 12) +
                          (*VRTETH3 11) (*VRTETH4 10) (*VRTETH5 9) +
                          (*VRTETH6 8) (*VRTETH7 7) (*VRTETH8 6) +
                          (*VRTETH9 5)) PROMPT('Port number')
 E3:         ELEM       TYPE(*CHAR) LEN(2) MIN(1) PROMPT('Source service +
                          access point')
             ELEM       TYPE(*INT2) DFT(*MAXFRAME) RANGE(265 8996) +
                          SPCVAL((*MAXFRAME 0)) PROMPT('SSAP maximum frame')
             ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) DFT(*CALC) +
                          VALUES(*CALC *NONSNA *SNA *HPR) +
                          PROMPT('SSAP type')
 E4:         ELEM       TYPE(*INT2) DFT(2) RANGE(0 99) +
                          PROMPT('Count limit')
             ELEM       TYPE(*INT2) DFT(5) RANGE(0 120) +
                          PROMPT('Time interval')
 Q1:         QUAL       TYPE(*NAME) LEN(10) MIN(1)
             QUAL       TYPE(*NAME) LEN(10) PROMPT('Library')
 PAGG:       PMTCTL     CTL(RSRCNAME) COND((*EQ *AGG))
 PNWS:       PMTCTL     CTL(RSRCNAME) COND((*EQ *NWSD))
