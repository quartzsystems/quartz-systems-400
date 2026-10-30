# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# ajar: create, list, extract and update zip and jar archives, the
# manifest, -C, verbose forms, messages and statuses.

probe create-list 'ajar cf a.jar data/fruits.txt data/sub && ajar tf a.jar'
probe dash 'ajar -cf a.jar data/fruits.txt && ajar -tf a.jar; ajar -c -f b.jar data/fruits.txt; echo "status $?"'
probe verbose 'ajar cvf a.jar data/fruits.txt data/sub data/words.txt; echo "--"; ajar tvf a.jar; echo "--"; mkdir out && cd out && ajar xvf ../a.jar'
probe manifest 'ajar cf a.jar data/fruits.txt && mkdir out && cd out && ajar xf ../a.jar META-INF/MANIFEST.MF && od -c META-INF/MANIFEST.MF'
probe no-manifest 'ajar cMf a.jar data/fruits.txt && ajar tf a.jar'
probe own-manifest 'printf "Main-Class: Hello\n" > man && ajar cmf man a.jar data/fruits.txt && mkdir out && cd out && ajar xf ../a.jar && od -c META-INF/MANIFEST.MF'
probe stored 'ajar c0vf a.jar data/words.txt && od -c a.jar | head -4'
probe update 'ajar cf a.jar data/fruits.txt && ajar uf a.jar data/nums.txt && ajar tf a.jar'
probe change-dir 'ajar cf a.jar -C data fruits.txt && ajar tf a.jar'
probe modes 'chmod 640 data/fruits.txt && ajar cf a.jar data/fruits.txt data/old.txt && mkdir out && cd out && ajar xf ../a.jar && ls -l data'
probe stdio 'ajar c data/fruits.txt | ajar t; echo "status $?"'
probe usage 'ajar; echo "status $?"; ajar vf a.jar; echo "status $?"; ajar cqf a.jar; echo "status $?"; ajar -h; echo "status $?"'
probe errors 'ajar tf data/fruits.txt; echo "status $?"; ajar tf nosuch.jar; echo "status $?"; ajar cf a.jar data/nosuch; echo "status $?"'
finish
