/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* TELNET: a 5250 session with another system from this display, over  */
/* TN5250 (RFC 2877 and 4777). A server that will not run 5250 ends the */
/* command with TCP2534; the VT and 3270 parameters are IBM's and are   */
/* checked, but only 5250 sessions are made.                            */
             CMD        PROMPT('Start TCP/IP TELNET') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC *IMOD) +
                          HLPPNLGRP(QHTGCMD) HLPID(STRTCPTELN) +
                          MSGF(QTCPMSG)
             PARM       KWD(RMTSYS) TYPE(*CHAR) LEN(255) +
                          SPCVAL((*INTNETADR)) MIN(1) +
                          PROMPT('Remote system')
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(64) PMTCTL(P1) +
                          PROMPT('Internet address')
             PARM       KWD(CTLCHAR) TYPE(*CHAR) LEN(1) DFT('&') +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Control character key')
             PARM       KWD(KBDTYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*LCL) SPCVAL((*LCL) (AGB) (AGE) (AGI) +
                          (AGM) (ALI) (ALM) (BGB) (BGE) (BLI) (BLM) +
                          (BRB) (BRE) (CAB) (CAE) (CAI) (CAM) (CLB) +
                          (CLE) (CSB) (CSE) (CYB) (DMB) (DME) (DMI) +
                          (DMM) (ESB) (FAB) (FAE) (FAI) (FAM) (FNB) +
                          (FNE) (FNI) (FNM) (FQB) (FQI) (GKB) (GNB) +
                          (GNE) (HIB) (HNB) (HNE) (ICB) (ICE) (ICI) +
                          (ICM) (INB) (INI) (IRB) (ITB) (ITE) (ITI) +
                          (ITM) (JEB) (JEI) (JKB) (JPB) (JPE) (JUB) +
                          (KAB) (KOB) (LAB) (LAE) (LTB) (LVB) (MKB) +
                          (MKE) (NCB) (NCE) (NEB) (NEE) (NEI) (NEM) +
                          (NWB) (NWE) (NWI) (NWM) (PKB) (PKE) (PLB) +
                          (PLE) (PRB) (PRE) (PRI) (PRM) (RCB) (RMB) +
                          (RME) (ROB) (ROE) (RUB) (RUE) (SFI) (SFM) +
                          (SGI) (SGM) (SKB) (SKE) (SPB) (SPE) (SPI) +
                          (SPM) (SQB) (SQE) (SSB) (SSE) (SSI) (SSM) +
                          (SWB) (SWE) (SWI) (SWM) (TAB) (THB) (THE) +
                          (TKB) (TKE) (TRB) (TRE) (UAB) (UAE) (UKB) +
                          (UKE) (UKI) (UKM) (USB) (USE) (USI) (USM) +
                          (VNB) (VNE) (YGI) (YGM) (*TRNTBL)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Keyboard language type')
             PARM       KWD(PAGEUP) TYPE(*INT2) RSTD(*YES) DFT(*PA2) +
                          SPCVAL((*PA2 26) (*PA1 25) (*PA3 27) +
                          (*NONE 0) (*F1 1) (*F2 2) (*F3 3) (*F4 4) +
                          (*F5 5) (*F6 6) (*F7 7) (*F8 8) (*F9 9) +
                          (*F10 10) (*F11 11) (*F12 12) (*F13 13) +
                          (*F14 14) (*F15 15) (*F16 16) (*F17 17) +
                          (*F18 18) (*F19 19) (*F20 20) (*F21 21) +
                          (*F22 22) (*F23 23) (*F24 24) (*CLEAR 28) +
                          (*ERASEINP 29) (*CSRSLT 30)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Page Up (Roll Down) key')
             PARM       KWD(PAGEDOWN) TYPE(*INT2) RSTD(*YES) DFT(*PA1) +
                          SPCVAL((*PA1 25) (*PA2 26) (*PA3 27) +
                          (*NONE 0) (*F1 1) (*F2 2) (*F3 3) (*F4 4) +
                          (*F5 5) (*F6 6) (*F7 7) (*F8 8) (*F9 9) +
                          (*F10 10) (*F11 11) (*F12 12) (*F13 13) +
                          (*F14 14) (*F15 15) (*F16 16) (*F17 17) +
                          (*F18 18) (*F19 19) (*F20 20) (*F21 21) +
                          (*F22 22) (*F23 23) (*F24 24) (*CLEAR 28) +
                          (*ERASEINP 29) (*CSRSLT 30)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Page Down (Roll Up) key')
             PARM       KWD(CSRSLT) TYPE(*INT2) RSTD(*YES) DFT(*NONE) +
                          SPCVAL((*NONE 0) (*F1 1) (*F2 2) (*F3 3) +
                          (*F4 4) (*F5 5) (*F6 6) (*F7 7) (*F8 8) +
                          (*F9 9) (*F10 10) (*F11 11) (*F12 12) +
                          (*F13 13) (*F14 14) (*F15 15) (*F16 16) +
                          (*F17 17) (*F18 18) (*F19 19) (*F20 20) +
                          (*F21 21) (*F22 22) (*F23 23) (*F24 24)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Cursor Select key')
             PARM       KWD(TBLVTOUT) TYPE(Q1) DFT(*CCSID) +
                          SNGVAL((*CCSID) (*DFT)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Outgoing EBCDIC/ASCII table')
             PARM       KWD(TBLVTIN) TYPE(Q2) DFT(*CCSID) +
                          SNGVAL((*CCSID) (*DFT)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Incoming ASCII/EBCDIC table')
             PARM       KWD(TBL3270OUT) TYPE(Q3) DFT(*KBDTYPE) +
                          SNGVAL((*KBDTYPE)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Outgoing 3270 mapping table')
             PARM       KWD(TBL3270IN) TYPE(Q4) DFT(*KBDTYPE) +
                          SNGVAL((*KBDTYPE)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Incoming 3270 mapping table')
             PARM       KWD(INZWAIT) TYPE(*INT2) DFT(120) +
                          RANGE(1 32767) SPCVAL((*NOMAX 0)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Timeout wait for host')
             PARM       KWD(NUMLCK) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NO) SPCVAL((*NO) (*YES)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Numeric lock keyboard')
             PARM       KWD(NULLS) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*BLANK) SPCVAL((*BLANK) (*REMOVE)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Handle nulls')
             PARM       KWD(TBLVTDRWO) TYPE(Q5) DFT(*DFT) +
                          SNGVAL((*DFT)) EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('ASCII full screen draw out')
             PARM       KWD(TBLVTDRWI) TYPE(Q6) DFT(*DFT) +
                          SNGVAL((*DFT)) EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('ASCII full screen draw in')
             PARM       KWD(VTOPT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NONE) +
                          VALUES(*LOCALECHO *AUTOWRAP *NEWLINE) +
                          SNGVAL((*NONE) (*ALL)) MAX(3) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('ASCII full screen options')
             PARM       KWD(DSPCHRATTR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Display character attributes')
             PARM       KWD(PAGESCROLL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('ASCII page scroll feature')
             PARM       KWD(ANSWERBACK) TYPE(*CHAR) LEN(20) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('ASCII answerback feature')
             PARM       KWD(TABSTOP) TYPE(*INT2) DFT(*DFT) +
                          RANGE(0 133) SNGVAL((*DFT 133) (*NONE 0)) +
                          MAX(10) PMTCTL(*PMTRQS) +
                          PROMPT('ASCII tab stops')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*MULTINAT) +
                          RANGE(1 65533) SPCVAL((*MULTINAT 84542540) +
                          (*BRITISH 84608077) (*DUTCH 84673614) +
                          (*FINNISH 84739151) (*FRENCH 18941008) +
                          (*FRENCHCAN 78119932) (*GERMAN 17368051) +
                          (*ITALIAN 19203060) (*NORDAN 85001299) +
                          (*SPANISH 78316543) (*SWEDISH 84935762) +
                          (*SWISS 78185469)) PMTCTL(*PMTRQS) +
                          PROMPT('Coded character set identifier')
             PARM       KWD(ASCOPRMOD) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*VT220B7) +
                          VALUES(*VT220B7 *VT220B8 *VT100 *VT52) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('ASCII operating mode ID')
             PARM       KWD(PORT) TYPE(*INT4) DFT(*DFT) RANGE(1 65534) +
                          SPCVAL((*DFT 0)) EXPR(*YES) PROMPT('Port')
             PARM       KWD(RMTVRTDSP) TYPE(*NAME) LEN(10) DFT(*DFT) +
                          SPCVAL((*DFT)) EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Remote virtual display')
             PARM       KWD(RMTUSER) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE) (*CURRENT)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) PROMPT('Remote user')
             PARM       KWD(RMTPWD) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) EXPR(*YES) PMTCTL(*PMTRQS) +
                          DSPINPUT(*NO) PROMPT('Remote password')
             PARM       KWD(RMTPWDENC) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*DES7) SPCVAL((*DES7) (*SHA1) (*NONE)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Remote password encryption')
             PARM       KWD(RMTINLPGM) TYPE(*NAME) LEN(10) +
                          DFT(*RMTUSRPRF) SPCVAL((*RMTUSRPRF) (*NONE)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Remote initial program')
             PARM       KWD(RMTINLMNU) TYPE(*NAME) LEN(10) +
                          DFT(*RMTUSRPRF) +
                          SPCVAL((*RMTUSRPRF) (*SIGNOFF)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Remote initial menu')
             PARM       KWD(RMTCURLIB) TYPE(*NAME) LEN(10) +
                          DFT(*RMTUSRPRF) SPCVAL((*RMTUSRPRF)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Remote current library')
             PARM       KWD(RMTKBDTYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*RMTSYS) SPCVAL((*RMTSYS) (*LCL)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Remote keyboard type')
             PARM       KWD(RMTCODPAG) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*RMTSYS) SPCVAL((*RMTSYS) (*LCL)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Remote codepage')
             PARM       KWD(RMTCHRSET) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*RMTSYS) SPCVAL((*RMTSYS) (*LCL)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Remote character set')
             PARM       KWD(SSL) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ENVVAR) VALUES(*ENVVAR *YES *NO) +
                          PROMPT('Secure connection')
             P1:        PMTCTL     CTL(RMTSYS) COND((*EQ *INTNETADR)) +
                          NBRTRUE(*EQ 1)
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q4:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q5:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q6:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             DEP        CTL((&RMTSYS *EQ *INTNETADR)) PARM(&INTNETADR) +
                          NBRTRUE(*EQ 1) MSGID(CPD0150)
             DEP        CTL((&RMTSYS *EQ *INTNETADR)) +
                          PARM((&INTNETADR *NE ' ')) NBRTRUE(*EQ 1) +
                          MSGID(TCP9095)
             DEP        CTL((&INTNETADR *NE ' ')) +
                          PARM((&RMTSYS *EQ *INTNETADR)) +
                          NBRTRUE(*EQ 1) MSGID(TCP9095)
             DEP        CTL(*ALWAYS) PARM((&KBDTYPE *EQ *TRNTBL) +
                          (&TBL3270OUT *EQ *KBDTYPE)) NBRTRUE(*NE 2) +
                          MSGID(TCP2511)
             DEP        CTL(*ALWAYS) PARM((&KBDTYPE *EQ *TRNTBL) +
                          (&TBL3270OUT *NE *KBDTYPE) +
                          (&TBL3270IN *EQ *KBDTYPE)) NBRTRUE(*NE 3) +
                          MSGID(TCP2511)
             DEP        CTL(*ALWAYS) PARM((&KBDTYPE *NE *TRNTBL) +
                          (&TBL3270OUT *NE *KBDTYPE)) NBRTRUE(*NE 2) +
                          MSGID(TCP2511)
             DEP        CTL(*ALWAYS) PARM((&KBDTYPE *NE *TRNTBL) +
                          (&TBL3270OUT *EQ *KBDTYPE) +
                          (&TBL3270IN *NE *KBDTYPE)) NBRTRUE(*NE 3) +
                          MSGID(TCP2511)
             DEP        CTL((&RMTUSER *NE ' ')) +
                          PARM((&RMTPWD *NE ' ')) NBRTRUE(*EQ 1) +
                          MSGID(TCP258A)
             DEP        CTL((&RMTPWD *NE ' ')) +
                          PARM((&RMTUSER *NE ' ')) NBRTRUE(*EQ 1) +
                          MSGID(TCP258A)
             DEP        CTL((&RMTUSER *EQ *NONE)) +
                          PARM((&RMTPWD *EQ *NONE)) NBRTRUE(*EQ 1) +
                          MSGID(TCP258B)
