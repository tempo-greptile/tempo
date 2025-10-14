#!/usr/bin/env python3
"""Strip ANSI color codes from bench.log"""

import re
from pathlib import Path

ANSI_ESCAPE = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')

def strip_colors_from_file(file_path: Path) -> None:
    """Remove ANSI color codes from a log file."""
    print(f"Stripping colors from {file_path}...")

    if not file_path.exists():
        print(f"Error: {file_path} not found")
        return

    # Read file
    with open(file_path, 'r') as f:
        content = f.read()

    # Strip colors
    clean_content = ANSI_ESCAPE.sub('', content)

    # Write back
    with open(file_path, 'w') as f:
        f.write(clean_content)

    print(f"✓ Colors stripped from {file_path}")
    print(f"  Original size: {len(content):,} bytes")
    print(f"  Cleaned size:  {len(clean_content):,} bytes")
    print(f"  Saved: {len(content) - len(clean_content):,} bytes")


if __name__ == "__main__":
    import sys

    if len(sys.argv) > 1:
        # Strip colors from specified files
        for file_arg in sys.argv[1:]:
            strip_colors_from_file(Path(file_arg))
    else:
        # Default: strip from bench.log
        strip_colors_from_file(Path("bench.log"))
