# Testing reth_bench_compare.py

## Running Tests

```bash
# Run all tests
python3 test_reth_bench_compare.py

# Run with verbose output
python3 test_reth_bench_compare.py -v

# Run specific test class
python3 test_reth_bench_compare.py TestUpdateRethRevisionRegex -v

# Run specific test method
python3 test_reth_bench_compare.py TestUpdateRethRevisionRegex.test_long_hash -v
```

## Test Coverage

The test suite covers:

1. **Time Parsing** (4 tests)
   - Milliseconds, microseconds, seconds
   - Invalid input handling

2. **ANSI Code Stripping** (4 tests)
   - Color codes, complex codes
   - Edge cases (empty strings, no codes)

3. **Timestamp Parsing** (4 tests)
   - Valid ISO timestamps
   - ANSI-wrapped timestamps
   - Invalid/missing timestamps

4. **Statistics** (4 tests)
   - Mean, median, min, max, std_dev
   - Edge cases (empty, single value, two values)

5. **Reth Revision Regex** (5 tests)
   - Standard format, long hashes
   - Variable whitespace
   - Multiple dependencies
   - Substitution logic

6. **Integration Tests** (2 tests)
   - Full update workflow
   - Bidirectional updates (short ↔ long hash)

**Total: 23 tests, all passing ✅**

## Test Structure

- Simple single-file approach
- No external dependencies required (uses Python's built-in `unittest`)
- Tests import functions directly from `reth_bench_compare.py`
