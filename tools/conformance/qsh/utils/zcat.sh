# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# zcat: files and standard input to standard output, names without .Z,
# messages.

probe files 'printf "one\n" | compress > one.Z && printf "two\n" | compress > two.Z && zcat one.Z two.Z; echo "status $?"; ls one* two*'
probe suffix 'printf "one\n" | compress > one.Z && zcat one; echo "status $?"'
probe stdin 'printf "piped\n" | compress | zcat'
probe missing 'zcat nosuch.Z; echo "status $?"; zcat data/fruits.txt; echo "status $?"'
probe options 'zcat -v x; echo "status $?"'
finish
