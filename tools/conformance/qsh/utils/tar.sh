# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# tar: key letters with and without a dash, create, append, update,
# list and extract, modifiers, verbose forms and messages.

probe create-list 'tar cf a.tar data/fruits.txt data/sub && tar tf a.tar'
probe dash 'tar -cf a.tar data/fruits.txt && tar -tf a.tar; tar -c -f b.tar data/fruits.txt && tar -t -f b.tar'
probe verbose 'tar cvf a.tar data/fruits.txt data/link data/old.txt; echo "--"; tar tvf a.tar; echo "--"; mkdir out && cd out && tar xvf ../a.tar'
probe verbose-streams 'tar cvf a.tar data/sub 2>/dev/null; echo "--"; mkdir out && cd out && tar xvf ../a.tar 2>/dev/null'
probe format 'tar cf a.tar data/fruits.txt && od -c a.tar | head -24 && wc -c < a.tar'
probe blocking 'tar cbf 1 a.tar data/fruits.txt && wc -c < a.tar; tar cfb b.tar 4 data/fruits.txt && wc -c < b.tar; tar cbf 0 c.tar data/fruits.txt; echo "status $?"'
probe stdio 'tar cf - data/fruits.txt | tar tvf -; tar c data/fruits.txt | tar t; echo "status $?"'
probe extract-named 'tar cf a.tar data && mkdir out && cd out && tar xf ../a.tar data/fruits.txt data/sub && find . | sort; tar xf ../a.tar "data/*.txt"; echo "status $?"'
probe modes 'chmod 4777 data/fruits.txt && tar cf a.tar data/fruits.txt data/old.txt && mkdir o1 o2 && (cd o1 && umask 022 && tar xf ../a.tar && ls -l data) && (cd o2 && umask 022 && tar xpmf ../a.tar && ls -l data)'
probe follow 'tar chf a.tar data/link && tar tvf a.tar'
probe links 'ln data/fruits.txt data/hard && tar cvf a.tar data/fruits.txt data/hard && tar tvf a.tar; tar clf b.tar data/fruits.txt; echo "status $?"'
probe append-update 'tar cf a.tar data/fruits.txt && tar rf a.tar data/nums.txt && tar tf a.tar && touch data/fruits.txt && tar uf a.tar data/fruits.txt data/nums.txt && tar tf a.tar'
probe confirm 'printf "y\nn\n" | tar cwf a.tar data/fruits.txt data/nums.txt; echo "status $?"; tar tf a.tar'
probe change-dir 'tar cCf data a.tar fruits.txt; echo "status $?"; tar tf a.tar; tar -C data -cf b.tar fruits.txt; echo "status $?"'
probe old-format 'tar cof a.tar data/fruits.txt; echo "status $?"; od -c a.tar | sed -n 17,18p'
probe usage 'tar; echo "status $?"; tar vf a.tar; echo "status $?"; tar cxf a.tar; echo "status $?"; tar cqf a.tar data; echo "status $?"; tar cf; echo "status $?"; tar cf a.tar; echo "status $?"'
probe missing 'tar cf a.tar data/nosuch data/fruits.txt; echo "status $?"; tar tf nosuch.tar; echo "status $?"; tar tf data/fruits.txt; echo "status $?"'
probe pax-interop 'tar cf a.tar data/sub && pax -v -f a.tar'
finish
