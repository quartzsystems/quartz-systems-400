# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# du: 512-byte blocks, -k, -s, -a, hard links, errors.

probe tree 'du data/sub'
probe kilo 'du -k data'
probe summary 'du -s data'
probe all 'du -a data/sub'
probe file 'du data/words.txt'
probe dot 'cd data/sub; du'
probe a-and-s 'du -a -s data'
probe missing 'du nosuch'
probe hard-link 'mkdir d; cp data/words.txt d/w; ln d/w d/h; du -a d'
probe several 'du -s data/sub data/sub/inner'
probe big 'i=0; while [ $i -lt 40 ]; do echo "a line of text that is long enough to count" >> big; i=$((i+1)); done; du big; du -k big; ls -s big'
finish
