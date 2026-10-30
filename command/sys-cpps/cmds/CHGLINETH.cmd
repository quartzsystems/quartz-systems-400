/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGLINETH: change an Ethernet line description; what is not given */
/* stays as it was.                                                   */
             CMD        PROMPT('Change Line Desc (Ethernet)') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDCLIN) HLPID(CHGLINETH)
             PARM       KWD(LIND) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Line description')
             PARM       KWD(RSRCNAME) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*AGG) (*NWSD)) +
                          PROMPT('Resource name')
             PARM       KWD(BRIDGE) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PROMPT('Bridge identifier')
             PARM       KWD(ONLINE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Online at IPL')
             PARM       KWD(VRYWAIT) TYPE(*INT2) DFT(*SAME) +
                          RANGE(15 180) SPCVAL((*NOWAIT 0) (*SAME -99)) +
                          PROMPT('Vary on wait')
             PARM       KWD(AGGPCY) TYPE(E1) DFT(*SAME) SNGVAL((*SAME)) +
                          PMTCTL(PAGG) PROMPT('Aggregate policy')
             PARM       KWD(AGGRSCL) TYPE(*NAME) LEN(10) MAX(8) +
                          DFT(*SAME) SPCVAL((*SAME)) PMTCTL(PAGG) +
                          PROMPT('Aggregated resource list')
             PARM       KWD(ASSOCPORT) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) +
                          PROMPT('Associated port resource name')
             PARM       KWD(ADPTADR) TYPE(*CHAR) LEN(12) DFT(*SAME) +
                          SPCVAL((*SAME) (*ADPT)) +
                          PROMPT('Local adapter address')
             PARM       KWD(EXCHID) TYPE(*CHAR) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME)) PMTCTL(*PMTRQS) +
                          PROMPT('Exchange identifier')
             PARM       KWD(GRPADR) TYPE(*CHAR) LEN(12) MAX(12) +
                          DFT(*SAME) SNGVAL((*SAME) (*NONE)) +
                          PROMPT('Group address')
             PARM       KWD(SSAP) TYPE(E3) MAX(24) DFT(*SAME) +
                          SNGVAL((*SAME) (*SYSGEN)) PROMPT('SSAP list')
             PARM       KWD(LINESPEED) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME 10M 100M 1G 10G 25G +
                          100G *AUTO) PROMPT('Line speed')
             PARM       KWD(DUPLEX) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *HALF *FULL *AUTO) +
                          PROMPT('Duplex')
             PARM       KWD(SRVOPT) TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) +
                          PROMPT('Serviceability options')
             PARM       KWD(MAXFRAME) TYPE(*INT2) DFT(*SAME) +
                          RANGE(1496 8996) SPCVAL((*SAME -99)) +
                          PROMPT('Maximum frame size')
             PARM       KWD(MAXCTL) TYPE(*INT2) DFT(*SAME) RANGE(1 256) +
                          SPCVAL((*SAME -99)) PMTCTL(*PMTRQS) +
                          PROMPT('Maximum controllers')
             PARM       KWD(AUTOCRTCTL) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PMTCTL(*PMTRQS) PROMPT('Autocreate controller')
             PARM       KWD(AUTODLTCTL) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 10000) SPCVAL((*NONE 0) (*SAME -99)) +
                          PMTCTL(*PMTRQS) PROMPT('Autodelete controller')
             PARM       KWD(CMNRCYLMT) TYPE(E4) DFT(*SAME) +
                          SNGVAL((*SAME -99) (*SYSVAL -1)) +
                          PROMPT('Recovery limits')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*SAME) SNGVAL((*SAME) +
                          (*SYSVAL) (*SYSOPR)) PROMPT('Message queue')
             PARM       KWD(THRESHOLD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *OFF *MIN *MED *MAX) +
                          PMTCTL(*PMTRQS) PROMPT('Error threshold level')
             PARM       KWD(GENTSTFRM) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PMTCTL(*PMTRQS) PROMPT('Generate test frame')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK '')) +
                          PROMPT('Text ''description''')
 E1:         ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) MIN(1) +
                          VALUES(*ETHCHL *LNKAGG) PROMPT('Standard')
             ELEM       TYPE(*CHAR) LEN(9) RSTD(*YES) MIN(1) +
                          VALUES(*DFT *SRCPORT *DESTPORT *SRCDESTP +
                          *RNDRBN) PROMPT('Policy type')
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
