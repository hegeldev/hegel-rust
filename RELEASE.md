RELEASE_TYPE: patch

Prebuilt `libhegel` binaries are no longer published for Intel macOS (`darwin/amd64`). The `macos-13` (x86_64) GitHub-hosted runners are scarce and routinely left the release job stuck for hours waiting for a runner, and we do not support Intel Macs. Apple-silicon macOS (`darwin/arm64`), Linux, and Windows binaries are unaffected; Intel-mac users can still build the `hegeltest-c` crate themselves.
