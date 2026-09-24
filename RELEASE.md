RELEASE_TYPE: patch

Fix compilation with the `static-engine` feature by enabling the `std` feature, which provides the allocator and panic runtime required to build its library artifacts.
