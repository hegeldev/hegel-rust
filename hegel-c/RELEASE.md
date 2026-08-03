RELEASE_TYPE: patch

This patch fixes a crash (SIGSEGV) that could occur when an application `dlopen`s libhegel, runs a test, and then `dlclose`s the library while a thread that called into it is still alive. It was most reliably triggered by a run that persists to the on-disk failure database.

Running the engine registers `std` thread-local destructors — and a global panic hook — whose code lives inside libhegel. glibc runs those thread-local destructors when the thread exits; if the library has already been unloaded by then, the destructor pointers dangle and the process crashes during teardown. libhegel is now built with `-z nodelete` on ELF platforms (Linux/Android), so `dlclose` keeps it resident and those destructors stay valid for the life of the process.
