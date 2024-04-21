#!/bin/bash -e
# Copyright (C) 2018 rerobots, Inc.
set -e

rm -f /etc/ssh/ssh_host_*
ssh-keygen -A

if [ -n "$HARDSHARE_LOG" ]; then
    exec /usr/sbin/sshd -d
else
    exec /usr/sbin/sshd -D
fi
