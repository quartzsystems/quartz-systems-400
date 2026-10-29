# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# ps: Qshell's default columns, -f, -l, -o keywords and headers,
# selection, messages. Only the probing job's own processes are listed.

probe default 'ps'
probe full 'ps -f'
probe long 'ps -l'
probe format 'ps -o pid,ppid,user,args'
probe renamed 'ps -o pid=ID -o comm=CMD'
probe no-headers 'ps -o pid= -o jobname='
probe pid 'ps -p $$'
probe user 'ps -u $LOGNAME -o pid,user'
probe job-columns 'ps -o jobnumber,jobuser,jobname,status,function'
probe time 'ps -o pid,time,etime'
probe bad-format 'ps -o nosuch'
probe bad-option 'ps -Z'
probe every 'ps -e -o pid | head -3'
probe all 'ps -A -o pid,device | head -3'
finish
