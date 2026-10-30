# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# pax: list, read, write and copy modes, formats, -s, -p, patterns,
# verbose listing columns, block sizes, messages and statuses.

probe write-list 'pax -w -f a.tar data/fruits.txt data/sub && pax -f a.tar'
probe verbose 'pax -w -f a.tar data/fruits.txt data/link data/old.txt && pax -v -f a.tar'
probe verbose-cpio 'pax -w -x cpio -f a.cpio data/fruits.txt data/sub && pax -v -f a.cpio'
probe dir-names 'pax -w -f a.tar data/sub && pax -f a.tar; pax -w -x cpio -f a.cpio data/sub && pax -f a.cpio'
probe sizes 'pax -w -f a.tar data/fruits.txt && wc -c < a.tar; pax -w -x cpio -f a.cpio data/fruits.txt && wc -c < a.cpio; pax -w -b 512 -f b.tar data/fruits.txt && wc -c < b.tar'
probe formats 'for x in ustar tar cpio sv4cpio sv4crc bcpio; do pax -w -x $x -f a.$x data/fruits.txt; echo "$x $?"; head -c 6 a.$x | od -c | head -1; done'
probe ustar-header 'pax -w -f a.tar data/fruits.txt && od -c a.tar | head -24'
probe cpio-header 'pax -w -x cpio -f a.cpio data/fruits.txt && od -c a.cpio | head -8'
probe read 'pax -w -f a.tar data && mkdir out && cd out && pax -r -f ../a.tar && find . | sort && ls -l data'
probe read-umask 'chmod 4777 data/fruits.txt && pax -w -f a.tar data/fruits.txt && mkdir o1 o2 && (cd o1 && umask 027 && pax -r -f ../a.tar && ls -l data) && (cd o2 && umask 027 && pax -r -pe -f ../a.tar && ls -l data)'
probe read-pm 'pax -w -f a.tar data/old.txt && mkdir out && cd out && pax -r -pm -f ../a.tar && ls -l data'
probe keep-update 'pax -w -f a.tar data/fruits.txt && echo x > data/fruits.txt && pax -r -k -f a.tar && cat data/fruits.txt && pax -r -u -f a.tar && cat data/fruits.txt'
probe verbose-read 'pax -w -f a.tar data/sub && mkdir out && cd out && pax -rv -f ../a.tar; echo "--"; pax -rv -f ../a.tar 2>/dev/null'
probe verbose-write 'pax -wv -f a.tar data/sub 2>&1'
probe patterns 'pax -w -f a.tar data && pax -f a.tar "data/s*"; pax -f a.tar data/sub; pax -c -f a.tar "data/*.txt"; pax -n -f a.tar "data/*.txt"'
probe no-match 'pax -w -f a.tar data/fruits.txt && pax -f a.tar nosuch; echo "status $?"'
probe subst 'pax -w -s ",^data/,new/," -f a.tar data/fruits.txt && pax -f a.tar; pax -s ",new,old,p" -f a.tar; pax -s ",/,_,g" -f a.tar; pax -s ",.*,," -f a.tar; echo "status $?"'
probe subst-bad 'pax -s "/a/b" -f a.tar; echo "status $?"; pax -s "/a/b/z" -f a.tar; echo "status $?"'
probe links 'ln data/fruits.txt data/hard && pax -w -f a.tar data/fruits.txt data/hard && pax -v -f a.tar && pax -w -x cpio -f a.cpio data/fruits.txt data/hard && pax -v -f a.cpio'
probe symlinks 'pax -w -f a.tar data/link && pax -v -f a.tar; pax -w -L -f b.tar data/link && pax -v -f b.tar'
probe no-descend 'pax -w -d -f a.tar data/sub && pax -f a.tar'
probe copy 'mkdir dest && pax -rw data/sub dest && find dest | sort; pax -rw -l data/fruits.txt dest && ls -l dest/data'
probe copy-errors 'pax -rw data/fruits.txt data/nums.txt; echo "status $?"; pax -rw data/fruits.txt nodir; echo "status $?"'
probe append 'pax -w -f a.tar data/fruits.txt && pax -w -a -f a.tar data/nums.txt && pax -f a.tar; pax -w -a data/nums.txt; echo "status $?"'
probe stdin-names 'printf "data/fruits.txt\ndata/nums.txt\n" | pax -w -f a.tar && pax -f a.tar'
probe stdio 'pax -w data/fruits.txt | pax -v'
probe missing 'pax -w -f a.tar data/nosuch data/fruits.txt; echo "status $?"; pax -f nosuch.tar; echo "status $?"'
probe not-archive 'pax -f data/fruits.txt; echo "status $?"; pax < /dev/null; echo "status $?"'
probe usage 'pax -q; echo "status $?"; pax -r -a; echo "status $?"; pax -w -x zip data; echo "status $?"; pax -w -b 1000 data; echo "status $?"'
probe interactive 'pax -w -f a.tar data/fruits.txt && echo "." | pax -r -i -f a.tar; echo "status $?"'
probe qsys 'pax -w -f a.tar /QSYS.LIB/QGPL.LIB/QCLSRC.FILE 2>&1 | head -3; pax -f a.tar | head -3'
finish
