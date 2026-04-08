#!/usr/bin/env python3
"""Merge multiple cargo llvm-cov --text outputs.

For each source file, takes the maximum execution count per line.
This allows combining coverage from parallel matrix jobs
(e.g., lib tests + mock_tenko + mock_dtako).

Usage:
    python3 scripts/merge_coverage.py part1.txt part2.txt ... > merged.txt
"""
import re
import sys
from collections import defaultdict


def parse_count(s):
    """Parse LLVM coverage count like '5', '1.19k', '2.50M'."""
    s = s.strip()
    if s.endswith(("k", "K")):
        return int(float(s[:-1]) * 1000)
    if s.endswith(("m", "M")):
        return int(float(s[:-1]) * 1_000_000)
    return int(s)


def parse_text_output(filepath):
    """Parse a cargo llvm-cov --text output file.

    Returns dict: source_path -> {line_num: (count_or_None, source_text)}
    """
    result = defaultdict(dict)
    current_file = None

    with open(filepath) as f:
        for raw_line in f:
            line = raw_line.rstrip("\n")

            # File header: /path/to/file.rs:
            m = re.match(r"^(/.*\.rs):$", line)
            if m:
                current_file = m.group(1)
                continue

            if current_file is None:
                continue

            # Executable line with count: "    1|      5| source code"
            # Count may have k/M suffix: "   99|  1.19k| ..."
            m = re.match(r"^(\s*\d+)\|\s*([0-9][0-9.]*[kKmM]?)\|(.*)$", line)
            if m:
                ln = int(m.group(1).strip())
                count = parse_count(m.group(2))
                source = m.group(3)
                if ln in result[current_file]:
                    old_count, _ = result[current_file][ln]
                    if old_count is not None:
                        count = max(count, old_count)
                result[current_file][ln] = (count, source)
                continue

            # Non-executable line: "    1|       | source code"
            m = re.match(r"^(\s*\d+)\|\s*\|(.*)$", line)
            if m:
                ln = int(m.group(1).strip())
                source = m.group(2)
                if ln not in result[current_file]:
                    result[current_file][ln] = (None, source)
                continue

    return result


def main():
    if len(sys.argv) < 2:
        print("Usage: merge_coverage.py <file1> [file2] ...", file=sys.stderr)
        sys.exit(1)

    merged = defaultdict(dict)

    for filepath in sys.argv[1:]:
        data = parse_text_output(filepath)
        for src_file, lines in data.items():
            for ln, (count, source) in lines.items():
                if ln in merged[src_file]:
                    old_count, old_source = merged[src_file][ln]
                    if count is not None and old_count is not None:
                        count = max(count, old_count)
                    elif count is None:
                        count = old_count
                    source = old_source  # keep first source text
                merged[src_file][ln] = (count, source)

    # Output in cargo llvm-cov --text format
    for src_file in sorted(merged.keys()):
        print(f"{src_file}:")
        lines = merged[src_file]
        for ln in sorted(lines.keys()):
            count, source = lines[ln]
            if count is None:
                print(f"{ln:>5}|       |{source}")
            else:
                print(f"{ln:>5}|{count:>7}|{source}")
        print()


if __name__ == "__main__":
    main()
