/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGTCPA: the stack's attributes. */
             CMD        PROMPT('Change TCP/IP Attributes') ALLOW(*ALL) +
                          HLPPNLGRP(QHTOCCMD) HLPID(CHGTCPA)
             PARM       KWD(TCPKEEPALV) TYPE(*INT4) DFT(*SAME) RANGE(1 +
                          40320) SPCVAL((*SAME -1) (*DFT 120)) +
                          PROMPT('Keep-alive interval (minutes)')
             PARM       KWD(TCPURGPTR) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*BSD) (*RFC)) +
                          PROMPT('Urgent pointer convention')
             PARM       KWD(TCPRCVBUF) TYPE(*INT4) DFT(*SAME) RANGE(512 +
                          8388608) SPCVAL((*SAME -1) (*DFT 65535)) +
                          PROMPT('Receive buffer (bytes)')
             PARM       KWD(TCPSNDBUF) TYPE(*INT4) DFT(*SAME) RANGE(512 +
                          8388608) SPCVAL((*SAME -1) (*DFT 65535)) +
                          PROMPT('Send buffer (bytes)')
             PARM       KWD(TCPR1CNT) TYPE(*INT4) DFT(*SAME) RANGE(1 +
                          15) SPCVAL((*SAME -1) (*DFT 3)) +
                          PROMPT('First retransmit threshold')
             PARM       KWD(TCPR2CNT) TYPE(*INT4) DFT(*SAME) RANGE(2 +
                          16) SPCVAL((*SAME -1) (*DFT 16)) +
                          PROMPT('Last retransmit threshold')
             PARM       KWD(TCPMINRTM) TYPE(*INT4) DFT(*SAME) RANGE(100 +
                          1000) SPCVAL((*SAME -1) (*DFT 250)) +
                          PROMPT('Shortest retransmit (ms)')
             PARM       KWD(TCPCLOTIMO) TYPE(*INT4) DFT(*SAME) RANGE(0 +
                          14400) SPCVAL((*SAME -1) (*DFT 120)) +
                          PROMPT('Closed connection wait (s)')
             PARM       KWD(TCPCNNMSG) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*THRESHOLD) (*ALL) +
                          (*NONE)) PROMPT('Messages for closed links')
             PARM       KWD(UDPCKS) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*YES) (*NO)) +
                          PROMPT('Check UDP checksums')
             PARM       KWD(IPPATHMTU) TYPE(E1) DFT(*SAME) +
                          SNGVAL((*SAME) (*DFT) (*NO)) +
                          PROMPT('Path MTU discovery')
             PARM       KWD(IPDTGFWD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*YES) (*NO)) +
                          PROMPT('Forward datagrams')
             PARM       KWD(IPSRCRTG) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*YES) (*NO)) +
                          PROMPT('Accept source routes')
             PARM       KWD(IPRSBTIMO) TYPE(*INT4) DFT(*SAME) RANGE(5 +
                          120) SPCVAL((*SAME -1) (*DFT 10)) +
                          PROMPT('Reassembly wait (seconds)')
             PARM       KWD(IPTTL) TYPE(*INT4) DFT(*SAME) RANGE(1 255) +
                          SPCVAL((*SAME -1) (*DFT 64)) +
                          PROMPT('Datagram lifetime (hops)')
             PARM       KWD(IPQOSENB) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*TOS) (*YES) +
                          (*NO)) PROMPT('Quality of service')
             PARM       KWD(IPQOSBCH) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*NORMAL) +
                          (*MINDELAY)) PMTCTL(*PMTRQS) +
                          PROMPT('QoS datagram batching')
             PARM       KWD(IPQOSTMR) TYPE(*INT4) DFT(*SAME) RANGE(5 +
                          5000) SPCVAL((*SAME -1) (*DFT 100)) +
                          PMTCTL(*PMTRQS) PROMPT('QoS timer (ms)')
             PARM       KWD(IPDEADGATE) TYPE(E2) DFT(*SAME) +
                          SNGVAL((*SAME) (*DFT) (*NO)) +
                          PROMPT('Gateway checks')
             PARM       KWD(ARPTIMO) TYPE(*INT4) DFT(*SAME) RANGE(1 +
                          1440) SPCVAL((*SAME -1) (*DFT 15)) +
                          PROMPT('ARP entry lifetime (minutes)')
             PARM       KWD(ECN) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*YES) (*NO)) +
                          PROMPT('Explicit congestion notice')
             PARM       KWD(NFC) TYPE(E3) SNGVAL((*DFT) (*CLEAR)) +
                          PROMPT('File cache for servers')
             PARM       KWD(LOGPCLERR) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*YES) (*NO)) +
                          PROMPT('Log protocol errors')
             PARM       KWD(DHCPUNQID) TYPE(E4) DFT(*SAME) +
                          SNGVAL((*SAME)) +
                          PROMPT('DHCP unique identifier')
             PARM       KWD(IP6ERRLMT) TYPE(*INT4) DFT(*SAME) RANGE(1 +
                          255) SPCVAL((*SAME -1) (*DFT 10)) +
                          PROMPT('ICMPv6 error burst')
             PARM       KWD(IP6ERRRATE) TYPE(*INT4) DFT(*SAME) RANGE(1 +
                          255) SPCVAL((*SAME -1) (*DFT 10)) +
                          PROMPT('ICMPv6 error rate (ms)')
             PARM       KWD(IP6HOPLMT) TYPE(*INT4) DFT(*SAME) RANGE(1 +
                          255) SPCVAL((*SAME -1) (*DFT 64)) +
                          PROMPT('IPv6 hop limit')
             PARM       KWD(IP6DUPRTLS) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*DFT) (*NONE) +
                          (*RTR)) +
                          PROMPT('IPv6 duplicate route sharing')
             PARM       KWD(IP6TMPAVL) TYPE(*DEC) LEN(6 1) DFT(*SAME) +
                          SPCVAL((*SAME -1.0) (*DFT 7.0)) +
                          PROMPT('Temporary address life (days)')
             PARM       KWD(IP6TMPAPL) TYPE(*DEC) LEN(6 1) DFT(*SAME) +
                          SPCVAL((*SAME -1.0) (*DFT 1.0)) +
                          PROMPT('Temporary address use (days)')
             PARM       KWD(IP6TMPAXP) TYPE(E5) DFT(*SAME) +
                          SNGVAL((*SAME) (*DFT)) MAX(5) +
                          PROMPT('Prefixes, no temp addresses')
             E1:        ELEM       TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          SPCVAL((*YES)) MIN(1) PROMPT('Discover')
                        ELEM       TYPE(*INT4) RANGE(5 40320) +
                          SPCVAL((*ONCE 0)) MIN(1) +
                          PROMPT('Rediscover every (minutes)')
             E2:        ELEM       TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          SPCVAL((*YES) (*LINE)) MIN(1) PROMPT('Check')
                        ELEM       TYPE(*INT4) RANGE(1 60) MIN(1) +
                          PROMPT('Every (minutes)')
             E3:        ELEM       TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*YES) (*NO)) +
                          PROMPT('Use the cache')
                        ELEM       TYPE(*INT4) DFT(*SAME) SPCVAL((*SAME +
                          -1) (*NOMAX 0)) +
                          PROMPT('Keep files (seconds)')
                        ELEM       TYPE(*INT4) DFT(*SAME) SPCVAL((*SAME +
                          -1)) PROMPT('Cache size (MB)')
             E4:        ELEM       TYPE(*CHAR) LEN(5) SPCVAL((*DUID) +
                          (*GEN)) PROMPT('Action')
                        ELEM       TYPE(*CHAR) LEN(60) SPCVAL((*PENDING +
                          000000000000000000000000000000000000000000000000000000000000)) +
                          PROMPT('Identifier')
             E5:        ELEM       TYPE(*CHAR) LEN(45) MIN(1) +
                          PROMPT('Prefix')
                        ELEM       TYPE(*INT4) RANGE(1 64) MIN(1) +
                          PROMPT('Prefix length')
