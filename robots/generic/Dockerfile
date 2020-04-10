# Base image using Ubuntu 18.04
#
#     docker build -t rerobots/hs-generic:latest -f Dockerfile .
#
#
# SCL <scott@rerobots.net>
# Copyright (C) 2018 rerobots, Inc.

FROM ubuntu:18.04

RUN apt-get update \
    && apt-get -y install \
       openssh-server \
       openvpn \
       avahi-daemon \
       nano \
    && mkdir -p /var/run/sshd

COPY init.sh /sbin/rerobots-hs-init.sh

CMD ["/sbin/rerobots-hs-init.sh"]
