RELEASE_TYPE: patch

The native backend (`--features native`) now rejects regex patterns nested beyond a fixed depth with a clear error, instead of overflowing the stack (and aborting the process) on pathologically nested groups.
