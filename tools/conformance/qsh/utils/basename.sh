# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# basename: the POSIX algorithm, suffixes, messages.

probe path 'basename /usr/bin/ls; basename /usr/lib/'
probe suffix 'basename /a/b/file.txt .txt; basename .txt .txt'
probe slashes 'basename ///; basename ""'
probe usage 'basename; basename a b c'
probe member 'basename /QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR .MBR'
probe dashes 'basename -- -x'
finish
