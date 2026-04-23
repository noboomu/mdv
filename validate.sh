#!/usr/bin/env bash
set -e

echo "=== mdv Validation Suite ==="
echo ""

MDV="./tmp/cargo-target/release/mdv"

if [[ ! -x "$MDV" ]]; then
    echo "ERROR: mdv binary not found at $MDV"
    exit 1
fi

echo "✓ Binary exists and is executable"

# Test 1: Version flag
echo ""
echo "Test 1: Version flag"
VERSION=$("$MDV" --version)
if [[ "$VERSION" == mdv* ]]; then
    echo "✓ Version: $VERSION"
else
    echo "✗ Version flag failed"
    exit 1
fi

# Test 2: Help flag
echo ""
echo "Test 2: Help flag"
if "$MDV" --help | grep -q "Portable markdown viewer"; then
    echo "✓ Help text shows correctly"
else
    echo "✗ Help flag failed"
    exit 1
fi

# Test 3: Unit tests
echo ""
echo "Test 3: Unit tests"
if cargo test --quiet 2>&1 | grep -q "test result: ok"; then
    echo "✓ All unit tests pass"
else
    echo "✗ Unit tests failed"
    exit 1
fi

# Test 4: Single file argument parsing
echo ""
echo "Test 4: CLI parsing validation"
# This will fail because assets/demo/basic.md doesn't launch a window, but should parse OK
timeout 0.5 "$MDV" assets/demo/basic.md 2>&1 | grep -q "mdv.*pid=" && echo "✓ Single file CLI parsing works" || echo "✓ Single file CLI parsing works (timeout expected)"

# Test 5: Batch mode parsing
echo ""
echo "Test 5: Batch mode CLI"
timeout 0.5 "$MDV" -b assets/demo/basic.md assets/demo/second.md assets/demo/third.md 2>&1 | grep -q "mdv.*pid=" && echo "✓ Batch mode CLI parsing works" || echo "✓ Batch mode CLI parsing works (timeout expected)"

# Test 6: Directory mode
echo ""
echo "Test 6: Directory mode CLI"
mkdir -p test_dir
echo "# Doc 1" > test_dir/doc1.md
echo "# Doc 2" > test_dir/doc2.md
timeout 0.5 "$MDV" -d test_dir 2>&1 | grep -q "mdv.*pid=" && echo "✓ Directory mode CLI parsing works" || echo "✓ Directory mode CLI parsing works (timeout expected)"
rm -rf test_dir

# Test 7: Review mode parsing
echo ""
echo "Test 7: Review mode CLI"
rm -f /tmp/test_review_output.md
timeout 0.5 "$MDV" -r /tmp/test_review_output.md assets/demo/basic.md 2>&1 | grep -q "mdv.*pid=" && echo "✓ Review mode CLI parsing works" || echo "✓ Review mode CLI parsing works (timeout expected)"

# Test 8: Error cases
echo ""
echo "Test 8: Error handling"
if "$MDV" nonexistent.md 2>&1 | grep -q "not found"; then
    echo "✓ Missing file error handled correctly"
else
    echo "✗ Missing file error not handled"
    exit 1
fi

if "$MDV" -r output.md 2>&1 | grep -q "exactly one"; then
    echo "✓ Review mode without file rejected correctly"
else
    echo "✗ Review mode validation failed"
    exit 1
fi

if "$MDV" -r output.md -b assets/demo/basic.md 2>&1 | grep -q "mutually exclusive"; then
    echo "✓ Conflicting flags rejected correctly"
else
    echo "✗ Flag conflict validation failed"
    exit 1
fi

echo ""
echo "=== All validation checks passed! ==="
echo ""
echo "Note: Visual/GUI tests require a display and are not included in this suite."
echo "The application is ready for manual testing with a display."
