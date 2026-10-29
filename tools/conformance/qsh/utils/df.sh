# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# df: the header, 512-byte and kilobyte blocks, file systems, errors.
# Sizes differ between systems; the layout is what is compared.

probe default 'df'
probe kilo 'df -k'
probe path 'df data'
probe posix 'df -P /'
probe several 'df data /tmp'
probe qsys-lib 'df /QSYS.LIB'
probe qopensys 'df /QOpenSys'
probe missing 'df nosuch'
probe bad-option 'df -z'
probe type 'df -t /'
finish
