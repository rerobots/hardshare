# Base image using Ubuntu 16.04
#
#     docker build -t hs.rerobots.net/generic:latest -f Dockerfile .
#
#
# SCL <scott@rerobots.net>
# Copyright (C) 2018 rerobots, Inc.

FROM ubuntu:xenial

RUN apt-get update \
    && apt-get -y install \
       openssh-server \
       openvpn \
       avahi-daemon \
       nano \
    && mkdir -p /var/run/sshd

COPY init.sh /sbin/rerobots-hs-init.sh

CMD ["/sbin/rerobots-hs-init.sh"]
