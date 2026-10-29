# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# cp: files, into directories, -R, -p, -i, -f, links, members, errors.

probe file 'cp data/fruits.txt f2; cat f2'
probe into-dir 'mkdir d; cp data/fruits.txt d; ls d'
probe several 'mkdir d; cp data/fruits.txt data/csv.txt d; ls d'
probe several-not-dir 'cp data/fruits.txt data/csv.txt data/words.txt'
probe directory-no-R 'cp data/sub x'
probe recursive 'cp -R data/sub s2; ls -R s2'
probe recursive-into 'mkdir d; cp -r data/sub d; ls -R d'
probe link-R 'mkdir d; cp -R data/link d; ls -l d'
probe link-follow 'cp data/link f; ls -l f'
probe preserve 'cp -p data/old.txt o; ls -l o'
probe no-preserve 'cp data/old.txt o; ls -l o | cut -c1-10'
probe umask 'chmod 666 data/fruits.txt; umask 077; cp data/fruits.txt f; ls -l f | cut -c1-10'
probe same-file 'cp data/fruits.txt data/fruits.txt'
probe missing 'cp data/nosuch x'
probe usage 'cp data/fruits.txt'
probe interactive-no 'cp data/csv.txt f; echo n | cp -i data/fruits.txt f; cat f'
probe interactive-yes 'cp data/csv.txt f; echo y | cp -i data/fruits.txt f; cat f'
probe force 'cp data/csv.txt f; chmod 444 f; cp -f data/fruits.txt f; cat f'
probe read-only 'cp data/csv.txt f; chmod 444 f; cp data/fruits.txt f'
probe member-to-stream 'cp /QSYS.LIB/QGPL.LIB/QCLSRC.FILE/QCLSRC.MBR m.txt 2>&1; ls -l m.txt'
probe missing-dir 'cp data/fruits.txt nodir/x'
finish
