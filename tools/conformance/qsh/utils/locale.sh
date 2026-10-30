# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# locale: the settings listing, -a, -m, keywords, categories, -c, -k, messages.

probe settings 'locale'
probe settings-env 'LC_TIME=POSIX locale; LC_ALL=POSIX locale'
probe a 'locale -a'
probe m 'locale -m | head'
probe keyword 'locale decimal_point; locale -k decimal_point grouping; locale -ck thousands_sep'
probe category 'locale LC_NUMERIC; locale -k LC_MESSAGES; locale -ck LC_TIME'
probe charmap 'locale charmap; locale -k LC_CTYPE'
probe unknown 'locale nosuch; echo $?'
probe usage 'locale -a -m; locale -z'
finish
