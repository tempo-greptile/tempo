#!/usr/bin/env python3
"""Unit tests for reth_bench_compare.py"""

import re
import tempfile
import unittest
from pathlib import Path
from datetime import datetime

# Import functions from the script
from reth_bench_compare import (
    parse_time_to_ms,
    strip_ansi_codes,
    parse_timestamp,
    compute_statistics,
    parse_log_file,
)


class TestParseTimeToMs(unittest.TestCase):
    """Test time string parsing to milliseconds."""

    def test_milliseconds(self):
        self.assertEqual(parse_time_to_ms("1.23ms"), 1.23)
        self.assertEqual(parse_time_to_ms("100ms"), 100.0)
        self.assertEqual(parse_time_to_ms("0.5ms"), 0.5)

    def test_microseconds(self):
        self.assertEqual(parse_time_to_ms("1000µs"), 1.0)
        self.assertEqual(parse_time_to_ms("500µs"), 0.5)
        self.assertEqual(parse_time_to_ms("123.45µs"), 0.12345)

    def test_seconds(self):
        self.assertEqual(parse_time_to_ms("1s"), 1000.0)
        self.assertEqual(parse_time_to_ms("0.5s"), 500.0)
        self.assertEqual(parse_time_to_ms("2.5s"), 2500.0)

    def test_invalid_input(self):
        self.assertIsNone(parse_time_to_ms("invalid"))
        self.assertIsNone(parse_time_to_ms(""))
        self.assertIsNone(parse_time_to_ms("123"))
        self.assertIsNone(parse_time_to_ms("ms"))


class TestStripAnsiCodes(unittest.TestCase):
    """Test ANSI escape code removal."""

    def test_no_ansi_codes(self):
        text = "Hello world"
        self.assertEqual(strip_ansi_codes(text), "Hello world")

    def test_color_codes(self):
        text = "\x1b[31mRed text\x1b[0m"
        self.assertEqual(strip_ansi_codes(text), "Red text")

    def test_complex_codes(self):
        text = "\x1b[1;32mBold green\x1b[0m normal"
        self.assertEqual(strip_ansi_codes(text), "Bold green normal")

    def test_empty_string(self):
        self.assertEqual(strip_ansi_codes(""), "")


class TestParseTimestamp(unittest.TestCase):
    """Test timestamp parsing from log lines."""

    def test_valid_timestamp(self):
        line = "2024-10-14T12:34:56.789Z Some log message"
        result = parse_timestamp(line)
        self.assertIsNotNone(result)
        self.assertIsInstance(result, datetime)
        self.assertEqual(result.year, 2024)
        self.assertEqual(result.month, 10)
        self.assertEqual(result.day, 14)

    def test_timestamp_with_ansi(self):
        line = "\x1b[32m2024-10-14T12:34:56.789Z\x1b[0m Some log message"
        result = parse_timestamp(line)
        self.assertIsNotNone(result)
        self.assertEqual(result.year, 2024)

    def test_no_timestamp(self):
        line = "This line has no timestamp"
        self.assertIsNone(parse_timestamp(line))

    def test_empty_line(self):
        self.assertIsNone(parse_timestamp(""))


class TestComputeStatistics(unittest.TestCase):
    """Test statistical computation."""

    def test_basic_statistics(self):
        times = [1.0, 2.0, 3.0, 4.0, 5.0]
        stats = compute_statistics(times)

        self.assertIsNotNone(stats)
        self.assertEqual(stats["count"], 5)
        self.assertEqual(stats["mean"], 3.0)
        self.assertEqual(stats["median"], 3.0)
        self.assertEqual(stats["min"], 1.0)
        self.assertEqual(stats["max"], 5.0)
        self.assertGreater(stats["std_dev"], 0)

    def test_single_value(self):
        times = [42.0]
        stats = compute_statistics(times)

        self.assertIsNotNone(stats)
        self.assertEqual(stats["count"], 1)
        self.assertEqual(stats["mean"], 42.0)
        self.assertEqual(stats["median"], 42.0)
        self.assertEqual(stats["std_dev"], 0.0)

    def test_empty_list(self):
        self.assertIsNone(compute_statistics([]))

    def test_two_values(self):
        times = [10.0, 20.0]
        stats = compute_statistics(times)

        self.assertIsNotNone(stats)
        self.assertEqual(stats["count"], 2)
        self.assertEqual(stats["mean"], 15.0)
        self.assertEqual(stats["median"], 15.0)


class TestUpdateRethRevisionRegex(unittest.TestCase):
    """Test the regex pattern used in update_reth_revision."""

    def setUp(self):
        """Set up test pattern."""
        self.pattern = r'(git\s*=\s*"https://github\.com/paradigmxyz/reth",\s*rev\s*=\s*")([^"]*)(")'

    def test_standard_format(self):
        """Test standard Cargo.toml format."""
        content = 'reth = { git = "https://github.com/paradigmxyz/reth", rev = "1619408" }'
        matches = re.findall(self.pattern, content)

        self.assertEqual(len(matches), 1)
        self.assertEqual(matches[0][1], "1619408")

    def test_long_hash(self):
        """Test with full 40-character commit hash."""
        content = 'reth = { git = "https://github.com/paradigmxyz/reth", rev = "d2070f4de34f523f6097ebc64fa9d63a04878055" }'
        matches = re.findall(self.pattern, content)

        self.assertEqual(len(matches), 1)
        self.assertEqual(matches[0][1], "d2070f4de34f523f6097ebc64fa9d63a04878055")

    def test_variable_whitespace(self):
        """Test pattern handles variable whitespace around equals signs."""
        # Note: comma stays next to URL closing quote (as in real Cargo.toml)
        content = 'reth = { git  =  "https://github.com/paradigmxyz/reth",  rev  =  "abc1234" }'
        matches = re.findall(self.pattern, content)

        self.assertEqual(len(matches), 1)
        self.assertEqual(matches[0][1], "abc1234")

    def test_multiple_dependencies(self):
        """Test matching multiple reth dependencies."""
        content = '''
        reth = { git = "https://github.com/paradigmxyz/reth", rev = "1619408" }
        reth-db = { git = "https://github.com/paradigmxyz/reth", rev = "1619408" }
        reth-evm = { git = "https://github.com/paradigmxyz/reth", rev = "1619408" }
        '''
        matches = re.findall(self.pattern, content)

        self.assertEqual(len(matches), 3)
        self.assertTrue(all(m[1] == "1619408" for m in matches))

    def test_substitution(self):
        """Test that substitution works correctly."""
        content = 'reth = { git = "https://github.com/paradigmxyz/reth", rev = "old123" }'
        new_commit = "new456"

        updated, count = re.subn(self.pattern, r"\g<1>" + new_commit + r"\g<3>", content)

        self.assertEqual(count, 1)
        self.assertIn('rev = "new456"', updated)
        self.assertNotIn('rev = "old123"', updated)


class TestUpdateRethRevisionIntegration(unittest.TestCase):
    """Integration tests for update_reth_revision using temporary files."""

    def setUp(self):
        """Create temporary Cargo.toml for testing."""
        self.test_dir = tempfile.mkdtemp()
        self.cargo_toml = Path(self.test_dir) / "Cargo.toml"

        # Create a minimal test Cargo.toml
        self.cargo_toml.write_text('''[workspace.dependencies]
reth = { git = "https://github.com/paradigmxyz/reth", rev = "1619408" }
reth-db = { git = "https://github.com/paradigmxyz/reth", rev = "1619408" }
reth-evm = { git = "https://github.com/paradigmxyz/reth", rev = "1619408" }
''')

    def test_update_to_new_commit(self):
        """Test updating all reth dependencies to a new commit."""
        pattern = r'(git\s*=\s*"https://github\.com/paradigmxyz/reth",\s*rev\s*=\s*")([^"]*)(")'
        new_commit = "d2070f4de34f523f6097ebc64fa9d63a04878055"

        original = self.cargo_toml.read_text()
        updated, count = re.subn(pattern, r"\g<1>" + new_commit + r"\g<3>", original)

        self.assertEqual(count, 3)
        self.assertIn(f'rev = "{new_commit}"', updated)
        self.assertNotIn('rev = "1619408"', updated)

    def test_bidirectional_update(self):
        """Test updating back and forth between commits."""
        pattern = r'(git\s*=\s*"https://github\.com/paradigmxyz/reth",\s*rev\s*=\s*")([^"]*)(")'

        # Update to long hash
        original = self.cargo_toml.read_text()
        long_hash = "d2070f4de34f523f6097ebc64fa9d63a04878055"
        updated_1, count_1 = re.subn(pattern, r"\g<1>" + long_hash + r"\g<3>", original)
        self.assertEqual(count_1, 3)

        # Update back to short hash
        short_hash = "1619408"
        updated_2, count_2 = re.subn(pattern, r"\g<1>" + short_hash + r"\g<3>", updated_1)
        self.assertEqual(count_2, 3)

        # Should be back to original
        self.assertEqual(updated_2, original)


class TestBlockFiltering(unittest.TestCase):
    """Test block filtering logic in parse_log_file."""

    def setUp(self):
        """Create a temporary log file for testing."""
        self.test_dir = tempfile.mkdtemp()
        self.log_file = Path(self.test_dir) / "test.log"

    def create_log_with_blocks(self, blocks_data):
        """Helper to create a log file with specified blocks.

        blocks_data: list of (block_number, tx_count, build_time_ms, state_root_time_ms, block_added_time_ms)
        """
        lines = []
        base_time = datetime(2025, 10, 14, 12, 0, 0)

        for block_num, tx_count, build_ms, state_root_ms, block_added_ms in blocks_data:
            parent_num = block_num - 1
            # Format timestamp with microsecond precision
            timestamp = base_time.replace(second=block_num, microsecond=0).strftime('%Y-%m-%dT%H:%M:%S.%f') + "Z"

            # Built payload log (matches actual format with parent_hash)
            lines.append(
                f'{timestamp}  INFO build_payload{{id=0x0a2a8876958e3746 parent_number={parent_num} '
                f'parent_hash=0xtest}}: Built payload sealed_block_header=SealedHeader {{ ... }} '
                f'total_transactions={tx_count} payment_transactions=0 elapsed={build_ms}ms '
                f'execution_elapsed=1.916µs builder_finish_elapsed=4.611833ms\n'
            )

            # Received block log
            timestamp2 = base_time.replace(second=block_num, microsecond=100000).strftime('%Y-%m-%dT%H:%M:%S.%f') + "Z"
            lines.append(
                f'{timestamp2}  INFO Received block from consensus engine number={block_num} '
                f'hash=0xtest{block_num:016x}\n'
            )

            # State root task finished log
            timestamp3 = base_time.replace(second=block_num, microsecond=200000).strftime('%Y-%m-%dT%H:%M:%S.%f') + "Z"
            lines.append(
                f'{timestamp3}  INFO State root task finished state_root=0xtest '
                f'elapsed={state_root_ms}ms\n'
            )

            # Block added log
            timestamp4 = base_time.replace(second=block_num, microsecond=300000).strftime('%Y-%m-%dT%H:%M:%S.%f') + "Z"
            lines.append(
                f'{timestamp4}  INFO Block added to canonical chain number={block_num} '
                f'hash=0xtest{block_num:016x} peers=0 txs={tx_count} gas_used=0.00Kgas '
                f'gas_throughput=0.00Kgas/second gas_limit=500.00Mgas full=0.0% '
                f'base_fee=0.00Gwei blobs=0 excess_blobs=0 elapsed={block_added_ms}ms\n'
            )

        self.log_file.write_text(''.join(lines))

    def test_filters_block_one(self):
        """Test that block #1 is filtered out."""
        self.create_log_with_blocks([
            (1, 5, 10.0, 5.0, 8.0),  # Block 1 should be filtered
            (2, 5, 12.0, 6.0, 9.0),  # Block 2 should be included
        ])

        build_times, state_root_times, payload_times, block_added_times = parse_log_file(self.log_file)

        # Block 1 should be filtered out, only block 2 included
        self.assertEqual(len(build_times), 1)
        self.assertEqual(len(state_root_times), 1)
        self.assertEqual(len(block_added_times), 1)

    def test_filters_low_tx_blocks(self):
        """Test that blocks with ≤1 transaction are filtered out."""
        self.create_log_with_blocks([
            (2, 0, 10.0, 5.0, 8.0),  # 0 txs - should be filtered
            (3, 1, 11.0, 5.5, 8.5),  # 1 tx - should be filtered
            (4, 2, 12.0, 6.0, 9.0),  # 2 txs - should be included
            (5, 5, 13.0, 6.5, 9.5),  # 5 txs - should be included
        ])

        build_times, state_root_times, payload_times, block_added_times = parse_log_file(self.log_file)

        # Only blocks 4 and 5 should be included
        self.assertEqual(len(build_times), 2)
        self.assertEqual(len(state_root_times), 2)
        self.assertEqual(len(block_added_times), 2)

        # Verify correct values
        self.assertAlmostEqual(build_times[0], 12.0, places=1)
        self.assertAlmostEqual(build_times[1], 13.0, places=1)

    def test_respects_block_range(self):
        """Test that block_range parameter correctly filters blocks."""
        self.create_log_with_blocks([
            (2, 5, 10.0, 5.0, 8.0),  # Outside range
            (3, 5, 11.0, 5.5, 8.5),  # Inside range
            (4, 5, 12.0, 6.0, 9.0),  # Inside range
            (5, 5, 13.0, 6.5, 9.5),  # Outside range
        ])

        # Only include blocks 3-4
        build_times, state_root_times, payload_times, block_added_times = parse_log_file(
            self.log_file, block_range=(3, 4)
        )

        self.assertEqual(len(build_times), 2)
        self.assertEqual(len(state_root_times), 2)
        self.assertEqual(len(block_added_times), 2)

        # Verify correct values (blocks 3 and 4)
        self.assertAlmostEqual(build_times[0], 11.0, places=1)
        self.assertAlmostEqual(build_times[1], 12.0, places=1)

    def test_state_root_filtering_consistency(self):
        """Test that state root times are filtered consistently with other metrics."""
        self.create_log_with_blocks([
            (1, 5, 10.0, 5.0, 8.0),   # Block 1 - filtered
            (2, 1, 11.0, 5.5, 8.5),   # Low tx - filtered
            (3, 5, 12.0, 6.0, 9.0),   # Included
            (4, 5, 13.0, 6.5, 9.5),   # Included
        ])

        build_times, state_root_times, payload_times, block_added_times = parse_log_file(self.log_file)

        # All metrics should have the same count (blocks 3 and 4)
        self.assertEqual(len(build_times), 2)
        self.assertEqual(len(state_root_times), 2)
        self.assertEqual(len(block_added_times), 2)

        # State root times should match the specified values for blocks 3 and 4
        self.assertAlmostEqual(state_root_times[0], 6.0, places=1)
        self.assertAlmostEqual(state_root_times[1], 6.5, places=1)

    def test_combined_filters(self):
        """Test combination of block #1 filter, tx filter, and range filter."""
        self.create_log_with_blocks([
            (1, 10, 10.0, 5.0, 8.0),   # Block 1 - filtered
            (2, 1, 11.0, 5.5, 8.5),    # Low tx - filtered
            (3, 5, 12.0, 6.0, 9.0),    # Outside range - filtered
            (4, 5, 13.0, 6.5, 9.5),    # Inside range - included
            (5, 5, 14.0, 7.0, 10.0),   # Inside range - included
            (6, 5, 15.0, 7.5, 10.5),   # Outside range - filtered
        ])

        build_times, state_root_times, payload_times, block_added_times = parse_log_file(
            self.log_file, block_range=(4, 5)
        )

        # Only blocks 4 and 5 should pass all filters
        self.assertEqual(len(build_times), 2)
        self.assertEqual(len(state_root_times), 2)
        self.assertEqual(len(block_added_times), 2)

    def test_empty_log_file(self):
        """Test handling of empty log file."""
        self.log_file.write_text("")

        build_times, state_root_times, payload_times, block_added_times = parse_log_file(self.log_file)

        self.assertEqual(len(build_times), 0)
        self.assertEqual(len(state_root_times), 0)
        self.assertEqual(len(block_added_times), 0)

    def test_all_metrics_collected_per_block(self):
        """Test that all four metrics are collected for each included block."""
        self.create_log_with_blocks([
            (2, 5, 10.0, 5.0, 8.0),
            (3, 5, 11.0, 5.5, 8.5),
        ])

        build_times, state_root_times, payload_times, block_added_times = parse_log_file(self.log_file)

        # Should have 2 measurements for each metric
        self.assertEqual(len(build_times), 2, "Should have 2 build times")
        self.assertEqual(len(state_root_times), 2, "Should have 2 state root times")
        self.assertEqual(len(payload_times), 2, "Should have 2 payload-to-received times")
        self.assertEqual(len(block_added_times), 2, "Should have 2 block added times")


if __name__ == "__main__":
    unittest.main()
