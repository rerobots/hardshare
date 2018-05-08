#!/bin/sh -e
# bootstrap a Linux workstation into a single-host workspace that uses
# LXD as provider.
#
#
# SCL <scott@rerobots.net>
# Copyright (C) 2018 rerobots, Inc.
set -e

sudo apt-get -y install python3-virtualenv
sudo apt -y install -t xenial-backports lxd lxd-client


if ! grep \^rra /etc/passwd; then
    sudo useradd -U -s /bin/sh -G lxd,dialout,plugdev -m rra
else
    echo 'WARNING: `rra` user already exists.'
    exit
fi
sudo usermod -L rra

export HOME=/home/rra
cd $HOME
sudo -u rra python3 -m virtualenv -p python3 PY3
sudo -u rra bash -c "source ${HOME}/PY3/bin/activate && pip install -U pip && pip install requests pyjwt"
sudo -u rra mkdir -p /home/rra/.ssh
sudo -u rra sh -c "echo 'StrictHostKeyChecking yes' > /home/rra/.ssh/config"
