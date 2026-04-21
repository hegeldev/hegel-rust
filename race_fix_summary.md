## Race Fix Investigation Summary

**Root Cause**: Non-atomic stream_id increment in hegel-core Connection.new_stream()

**The Race**:
```python
# hegel-core/src/hegel/protocol/connection.py lines 215-216
stream_id = StreamId(self.__next_stream_id << 1)  # Read
self.__next_stream_id += 1                        # Write (separate bytecode)
```

**Chain of Events**:
1. Two ThreadPoolExecutor workers call new_stream() concurrently
2. Both read same __next_stream_id value → allocate same stream_id
3. Both test cases send packets on the colliding stream_id
4. First client sends CLOSE_STREAM → server marks stream closed
5. Second client sends regular packet → hits `assert not stream.closed`
6. AssertionError escapes _reader_loop → finally: self.close()
7. Transport writer closed → other threads get ValueError on write

**Fix Applied**:
```python
with self.__writer_lock:
    stream_id = StreamId(self.__next_stream_id << 1)
    self.__next_stream_id += 1
```

**Evidence**:
- race_unit_test.py: 20/20 trials reproduced stream_id collision
- CI workflow test-race-fix.yml: tests the fix on the known failing configuration

**Status**: Waiting for CI run f39a349 to verify the fix works.
