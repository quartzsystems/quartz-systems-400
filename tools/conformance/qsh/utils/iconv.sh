# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# iconv: the code set names Qshell takes, conversions, -l, messages.

probe to-ebcdic 'printf AB | iconv -f UTF-8 -t IBM-037 | od -t x1'
probe from-ebcdic 'printf "\301\302\045" | iconv -f IBM-037 -t UTF-8'
probe ccsid-names 'printf hi | iconv -f IBMCCSID01208 -t IBMCCSID00037 | od -t x1'
probe numbers 'printf hi | iconv -f 1208 -t 37 | od -t x1'
probe cp-names 'printf hi | iconv -f UTF-8 -t CP037 | od -t x1'
probe latin1 'printf "\351\n" | iconv -f ISO8859-1 -t UTF-8 | od -t x1'
probe unknown 'echo x | iconv -f NOPE -t UTF-8'
probe usage 'echo x | iconv -t UTF-8'
probe missing 'iconv -f UTF-8 -t 37 data/nosuch'
probe list 'iconv -l | head -5'
finish
