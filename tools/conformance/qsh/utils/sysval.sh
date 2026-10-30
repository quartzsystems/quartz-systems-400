# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# sysval: values, -p, network attributes, messages.

probe one 'sysval QCCSID'
probe several 'sysval QCCSID QDATFMT QSECURITY'
probe names 'sysval -p QCCSID QMODEL'
probe list 'sysval QSYSLIBL'
probe date 'sysval QDATE; sysval QTIME'
probe net 'sysval -n SYSNAME; sysval -n LCLNETID'
probe net-names 'sysval -np SYSNAME'
probe unknown 'sysval QNOSUCH; echo $?'
probe net-unknown 'sysval -n NOSUCH; echo $?'
probe usage 'sysval; echo $?; sysval -x Q; echo $?'
finish
