# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# logger: messages only; nothing is written to the history log.

probe bad-option 'logger -x hi; echo $?'
probe bad-priority 'logger -p bogus.info hi; echo $?'
probe missing-file 'logger -f data/nosuch; echo $?'
finish
