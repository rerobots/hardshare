Internals
=========

Summary
-------

The following sections show advanced configuration options and internal details
of the hardshare client. Most users can safely skip this.


SSH Keys
--------

An SSH key is required to create SSH tunnels through which remote users connect
to containers that you host. This section describes how to manually create keys
and some security considerations. Because a key pair is created as part of a new
configuration (``hardshare config -c``) automatically, this section can be
skipped unless something breaks.

There might already be an SSH key at ``~/.ssh/id_rsa``. If not, or if you want to
create a new pair for this purpose, then::

  ssh-keygen

to start an interactive process to create a new pair. The default options are
sufficient here; the prompt "default" is selected by simply pushing ENTER
without typing text. For convenience, we recommend that you do not create a
password for the key. If you insist, then managing such a key is discussed
below. Additional instructions about creating and working with SSH keys, for
example from DigitalOcean_ or GitHub_.

The SSH key is used by the hardshare client in a way that does not motivate
adding password protection: to create reverse tunnels from rerobots-managed
servers into containers that you host. Only the public key is copied to the
rerobots server-side. Furthermore, :ref:`API tokens <ssec:api-tokens>` provide
for authentication and authorization of the hardshare client with respect to
your rerobots account. In summary, this SSH key has a technical role and
provides for encryption, but exposure risk of the secret key small.

If the SSH key has a password, then there must be some way for the hardshare
client to use the key without having to know the password. For this, you should
configure ``ssh-agent``, usage of which is presented in the `OpenBSD manual`_.
If you are new to ``ssh-agent``, we recommend reading about basic ideas of how
it works at http://www.unixwiz.net/techtips/ssh-agent-forwarding.html

Finally, add the SSH secret key path::

  hardshare config --add-ssh-path path/to/your/ssh_key


.. _GitHub: https://help.github.com/en/github/authenticating-to-github/connecting-to-github-with-ssh
.. _DigitalOcean: https://www.digitalocean.com/community/tutorials/how-to-set-up-ssh-keys--2
.. _OpenBSD manual: http://man.openbsd.org/OpenBSD-current/man1/ssh-agent.1
