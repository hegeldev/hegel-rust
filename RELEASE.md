RELEASE_TYPE: patch

The native backend (`--features native`) now writes example-database values atomically (temp file plus rename), so a process sharing the database directory can't observe a partially-written value.
