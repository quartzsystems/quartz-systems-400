# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# chmod: octal and symbolic modes, -R, errors, the mode string.

probe octal 'chmod 600 data/fruits.txt; ls -l data/fruits.txt'
probe u-plus-x 'chmod u+x data/fruits.txt; ls -l data/fruits.txt'
probe go-minus-r 'chmod go-r data/fruits.txt; ls -l data/fruits.txt'
probe a-equals 'chmod a=rw data/fruits.txt; ls -l data/fruits.txt'
probe plus-x-umask 'umask 027; chmod +x data/fruits.txt; ls -l data/fruits.txt'
probe equals-r 'chmod =r data/fruits.txt; ls -l data/fruits.txt'
probe clauses 'chmod u=rwx,g=rx,o= data/fruits.txt; ls -l data/fruits.txt'
probe copy-bits 'chmod 654 data/fruits.txt; chmod u=g data/fruits.txt; ls -l data/fruits.txt'
probe setuid 'chmod 4755 data/fruits.txt; ls -l data/fruits.txt'
probe setgid-S 'chmod 2644 data/fruits.txt; ls -l data/fruits.txt'
probe sticky 'chmod 1777 data/sub; ls -ld data/sub; chmod 1644 data/fruits.txt; ls -l data/fruits.txt'
probe big-X 'chmod 644 data/fruits.txt; chmod a+X data/fruits.txt data/sub; ls -l data/fruits.txt; ls -ld data/sub'
probe minus-w 'chmod -w data/fruits.txt; ls -l data/fruits.txt'
probe recursive 'chmod -R go= data/sub; ls -lR data/sub'
probe bad-octal 'chmod 999 data/fruits.txt'
probe bad-symbolic 'chmod u+q data/fruits.txt'
probe no-file 'chmod 644'
probe missing 'chmod 644 data/nosuch'
probe link 'chmod 600 data/link; ls -l data/fruits.txt data/link'
finish
