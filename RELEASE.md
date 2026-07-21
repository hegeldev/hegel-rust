RELEASE_TYPE: patch

This patch removes the background thread that drove each test run's engine. The engine now runs on the same thread as the test itself, resumed each time the test asks for its next test case. Test behaviour is unchanged; the `hegel-worker` thread simply no longer exists (e.g. in debugger thread lists), and each test case costs two fewer thread context switches. This is groundwork for supporting platforms without threads, such as WebAssembly.
