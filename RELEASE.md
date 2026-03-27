RELEASE_TYPE: patch

Move default hegel install to a versioned directory in $XDG_CACHE_HOME/hegel rather than .hegel/venv.

This fixes problems with workflows that share a project directory with a container and then end up broken in either the host or container because the virtualenv points to a python that doesn't exist.
