# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# setccsid: setting the CCSID of files, links, messages.

probe set 'setccsid 1208 data/fruits.txt; attr data/fruits.txt CCSID'
probe several 'setccsid 37 data/fruits.txt data/nums.txt; attr data/nums.txt CCSID'
probe link 'setccsid -h 1208 data/link; echo $?; attr -h data/link CCSID'
probe bad 'setccsid abc data/fruits.txt; echo $?; setccsid 0 data/fruits.txt; echo $?'
probe missing 'setccsid 37 data/nosuch; echo $?'
probe usage 'setccsid; echo $?; setccsid 37; echo $?'
finish
