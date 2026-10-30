/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGTCPDMN: the local names and the name servers. */
             CMD        PROMPT('Change TCP/IP Domain') ALLOW(*ALL) +
                          HLPPNLGRP(QHTOCCMD) HLPID(CHGTCPDMN)
             PARM       KWD(HOSTNAME) TYPE(*CHAR) LEN(63) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE ' ')) +
                          PROMPT('Local host name')
             PARM       KWD(DMNNAME) TYPE(*CHAR) LEN(255) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE ' ')) +
                          PROMPT('Local domain name')
             PARM       KWD(DMNSCHLIST) TYPE(*CHAR) LEN(256) DFT(*SAME) +
                          SPCVAL((*SAME) (*DFT ' ')) +
                          PROMPT('Domains to search')
             PARM       KWD(HOSTSCHPTY) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*REMOTE *LOCAL *SAME) +
                          PROMPT('Search the host table first')
             PARM       KWD(INTNETADR) TYPE(E1) PROMPT('Name servers')
             PARM       KWD(PORT) TYPE(*INT4) DFT(*SAME) RANGE(1 65535) +
                          SPCVAL((*SAME 0)) PMTCTL(*PMTRQS) +
                          PROMPT('Name server port')
             PARM       KWD(PROTOCOL) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*UDP *TCP *SAME) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Name server protocol')
             PARM       KWD(INLDMNSVR) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*FIRST *ROTATE *SAME) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('First name server asked')
             PARM       KWD(DMNSVRRTY) TYPE(E2) PMTCTL(*PMTRQS) +
                          PROMPT('Name server retries')
             PARM       KWD(DNSSEC) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*NO *YES *SAME) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Validate with DNSSEC')
             E1:        ELEM       TYPE(*CHAR) LEN(45) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE ' ')) PROMPT('First')
                        ELEM       TYPE(*CHAR) LEN(45) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE ' ')) PROMPT('Second')
                        ELEM       TYPE(*CHAR) LEN(45) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE ' ')) PROMPT('Third')
             E2:        ELEM       TYPE(*INT2) DFT(*SAME) RANGE(1 99) +
                          SPCVAL((*SAME 0)) PROMPT('Attempts')
                        ELEM       TYPE(*INT2) DFT(*SAME) RANGE(1 99) +
                          SPCVAL((*SAME 0)) PROMPT('Seconds between')
