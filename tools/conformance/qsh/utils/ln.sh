# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# ln: hard and symbolic links, -f, into directories, errors.

probe hard 'ln data/fruits.txt h; cat h; ls -l data/fruits.txt'
probe symbolic 'ln -s data/fruits.txt s; ls -l s; cat s'
probe dangling 'ln -s nowhere s; ls -l s; cat s'
probe into-dir 'mkdir d; ln -s ../data/fruits.txt d; ls -l d'
probe exists 'ln -s data/fruits.txt s; ln -s data/csv.txt s'
probe force 'ln -s data/fruits.txt s; ln -sf data/csv.txt s; cat s'
probe several-not-dir 'ln data/fruits.txt data/csv.txt x'
probe missing 'ln data/nosuch h'
probe usage 'ln x'
probe dir-hard 'ln data/sub h'
finish
