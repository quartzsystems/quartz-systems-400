# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# expand: tab stops, lists, backspace, messages.

probe default 'expand data/tabs.txt'
probe width 'expand -t 4 data/tabs.txt'
probe list 'printf "a\tb\tc\td\n" | expand -t 3,7'
probe past-list 'printf "a\tb\tc\td\n" | expand -t 2,4'
probe bad-list 'expand -t 4,2 data/tabs.txt; expand -t 0 data/tabs.txt'
probe backspace 'printf "ab\b\tc\n" | expand | od -c'
probe missing 'expand data/nosuch'
finish
