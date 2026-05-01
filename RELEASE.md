RELEASE_TYPE: patch

This patch fixes `ip_addresses()` returning malformed strings when generating mixed IPv4/IPv6 addresses (the default, with no version specified). The server returns a `[index, value]` pair for `one_of` schemas, but the generator was attempting to deserialize the entire pair as a string rather than extracting the value.
