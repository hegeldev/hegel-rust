#!/home/dev/hegel-core/.venv/bin/python3
"""Wrapper that reduces the GIL switch interval to maximize race probability."""
import sys

# Set the GIL switch interval very low to increase the chance of a
# context switch between the load and store of Connection.__next_stream_id
# in new_stream(). The default is 5ms; 100ns gives ~50000x more switching.
sys.setswitchinterval(0.0000001)

from hegel.__main__ import main

if __name__ == "__main__":
    sys.exit(main())
