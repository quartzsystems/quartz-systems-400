# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# rm: files, -r, -f, -i, the refused paths, errors.

probe file 'rm data/fruits.txt; ls data/fruits.txt'
probe missing 'rm data/nosuch'
probe force-missing 'rm -f data/nosuch; echo $?'
probe force-none 'rm -f; echo $?'
probe usage 'rm'
probe directory 'rm data/sub'
probe recursive 'rm -r data/sub; ls data'
probe interactive-no 'echo n | rm -i data/fruits.txt; ls data/fruits.txt'
probe interactive-yes 'echo y | rm -i data/fruits.txt; ls data/fruits.txt'
probe dot 'rm -r .'
probe dot-dot 'rm -r data/..'
probe link 'rm data/link; cat data/fruits.txt'
probe read-only 'chmod 444 data/csv.txt; rm data/csv.txt </dev/null; ls data/csv.txt'
probe force-interactive 'echo n | rm -f -i data/fruits.txt; echo $?'
finish
