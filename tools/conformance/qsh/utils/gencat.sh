# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# gencat: source syntax, merging, errors, and whether the catalog bytes are readable by dspmsg.

probe build 'printf '\''$ c\n1 one\\n\n2 two %%s\\n\n$set 2\n1 set two\\n\n'\'' > m.msg; gencat m.cat m.msg; echo $?; ls -l m.cat | cut -c1-10'
probe quote 'printf '\''$quote "\n1 "q \\" x\\n"\n'\'' > q.msg; gencat q.cat q.msg; dspmsg q.cat 1'
probe continuation 'printf '\''1 one \\\ntwo\\n\n'\'' > c.msg; gencat c.cat c.msg; dspmsg c.cat 1'
probe delete 'printf '\''1 one\n'\'' > a.msg; printf '\''1\n'\'' > b.msg; gencat x.cat a.msg b.msg; dspmsg x.cat 1 '\''gone\n'\'''
probe delset 'printf '\''$set 3\n1 three\n$delset 3\n'\'' > a.msg; gencat y.cat a.msg; dspmsg -s 3 y.cat 1 '\''deleted\n'\'''
probe merge 'printf '\''1 first\n'\'' > a.msg; printf '\''2 second\n'\'' > b.msg; gencat z.cat a.msg; gencat z.cat b.msg; dspmsg z.cat 1; echo; dspmsg z.cat 2; echo'
probe stdout 'printf '\''1 hi\n'\'' > a.msg; gencat - a.msg | od -c | head -3'
probe bad-line 'printf '\''x y\n'\'' > a.msg; gencat e.cat a.msg; echo $?'
probe bad-set 'printf '\''$set 0\n'\'' > a.msg; gencat e.cat a.msg; echo $?'
probe big-number 'printf '\''40000 x\n'\'' > a.msg; gencat e.cat a.msg; echo $?'
probe not-catalog 'printf '\''1 x\n'\'' > a.msg; gencat data/fruits.txt a.msg; echo $?'
probe usage 'gencat x.cat'
finish
