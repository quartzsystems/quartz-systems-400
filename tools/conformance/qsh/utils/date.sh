# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# date: the default format and zone (TZ unset, the system's QTIMZON),
# conversions, setting, messages. Times differ; the forms are compared.

probe default 'date'
probe utc 'date -u'
probe tz 'echo "TZ=[$TZ]"; TZ=EST5EDT date; TZ="<+09>-9" date "+%H %Z %z"'
probe letters 'date "+%a %A %b %B %d %e %j %m %y %C %u %w %U %W %V %G"'
probe time-letters 'date "+%I %p %M %S %r %R %T %D %F %n%t%%"'
probe epoch 'date "+%s"'
probe locale 'date "+%c|%x|%X"'
probe set 'date 010112002020'
probe too-many 'date +%Y +%m'
probe bad-option 'date -k'
probe unknown-letter 'date "+%Q"'
finish
