from pathlib import Path

import pytest

from hegel.conformance import conformance_tests, run_conformance_test

BUILD_DIR = Path(__file__).parent / "rust" / "target" / "release"

TESTS = conformance_tests({
    "booleans": BUILD_DIR / "test_booleans",
    "integers": BUILD_DIR / "test_integers",
    "floats": BUILD_DIR / "test_floats",
    "text": BUILD_DIR / "test_text",
    "binary": BUILD_DIR / "test_binary",
    "lists": BUILD_DIR / "test_lists",
    "sampled_from": BUILD_DIR / "test_sampled_from",
})


@pytest.mark.parametrize("test_name,binary_path", TESTS, ids=[t[0] for t in TESTS])
def test_conformance(test_name, binary_path):
    run_conformance_test(test_name, binary_path)
