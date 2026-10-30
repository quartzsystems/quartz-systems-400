# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# test: the program form (/usr/bin/test) and its messages.

probe files '/usr/bin/test -f data/fruits.txt; echo $?; /usr/bin/test -d data/fruits.txt; echo $?'
probe strings '/usr/bin/test abc = abc; echo $?; /usr/bin/test -z '\'''\''; echo $?'
probe numbers '/usr/bin/test 10 -gt 9; echo $?; /usr/bin/test x -eq 1; echo $?'
probe logic '/usr/bin/test \( -f nosuch -o -d data \) -a -n x; echo $?'
probe link '/usr/bin/test -h data/link; echo $?'
probe errors '/usr/bin/test -q x; echo $?; /usr/bin/test a = b c; echo $?'
probe find 'find data -name '\''*.txt'\'' -exec test -s {} \; -print | sort'
finish
