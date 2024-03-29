#!/bin/bash -e
# bootstrap a new Docker container for the generic host of hardshare clients
#
#
# SCL <scott@rerobots.net>
# Copyright (C) 2018 rerobots, Inc.
set -e

rm -f /etc/ssh/ssh_host_*
ssh-keygen -A

if [ -n "$HARDSHARE_LOG" ]; then
    exec /usr/sbin/sshd -d
else
    exec /usr/sbin/sshd -D
fi