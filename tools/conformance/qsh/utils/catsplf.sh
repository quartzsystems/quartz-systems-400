# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# catsplf: a spooled file of the probe's job, page forms, -a, -e, -p,
# messages. The spooled file is made and deleted in the job.

probe make 'system -i "DSPLIBL OUTPUT(*PRINT)"; echo $?'
probe job 'getjobid -s $$'
probe cat 'set -- $(getjobid -s $$); catsplf -j $3/$2/$1 QPRTSPLF "*LAST" | head -12'
probe eject 'set -- $(getjobid -s $$); catsplf -e -j $3/$2/$1 QPRTSPLF "*LAST" | od -c | head -6'
probe asa 'set -- $(getjobid -s $$); catsplf -p -j $3/$2/$1 QPRTSPLF "*LAST" | head -4'
probe all 'set -- $(getjobid -s $$); catsplf -a -j $3/$2/$1 | head -4'
probe missing 'set -- $(getjobid -s $$); catsplf -j $3/$2/$1 NOSUCH 1; echo $?'
probe no-job 'catsplf -j 999999/NOUSER/NOJOB QPRINT 1; echo $?'
probe usage 'catsplf; echo $?; catsplf -z; echo $?'
probe cleanup 'system -i -q "DLTSPLF FILE(QPRTSPLF) SPLNBR(*LAST)"; echo $?'
finish
