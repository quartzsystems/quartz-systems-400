# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# mv: renames, into directories, replacing, -i, -f, errors.

probe file 'mv data/fruits.txt f; ls data | grep -c fruits; cat f'
probe into-dir 'mkdir d; mv data/fruits.txt d; ls d'
probe several 'mkdir d; mv data/fruits.txt data/csv.txt d; ls d'
probe several-not-dir 'mv data/fruits.txt data/csv.txt data/words.txt'
probe directory 'mv data/sub s; ls -R s'
probe replace 'mv data/csv.txt data/fruits.txt; cat data/fruits.txt'
probe same-file 'mv data/fruits.txt data/fruits.txt'
probe missing 'mv data/nosuch x'
probe onto-dir 'mkdir -p d/fruits.txt; mv data/fruits.txt d'
probe interactive-no 'echo n | mv -i data/csv.txt data/fruits.txt; cat data/fruits.txt'
probe force 'mv -i -f data/csv.txt data/fruits.txt; cat data/fruits.txt'
probe usage 'mv x'
probe keeps-time 'mv data/old.txt o; ls -l o'
finish
