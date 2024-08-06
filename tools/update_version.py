#!/usr/bin/env python3
# coding:utf8 vim:ts=4

import sys
import re
import os


def should_update_version(line, in_dependencies):
    """
    判断是否应该更新版本
    """
    if not in_dependencies and re.search(r'^version\s*=\s*"([^"]+)"', line):
        return True
    if in_dependencies and (path_match := re.search(r'path\s*=\s*"([^"]+)"', line)) and re.search(r'version\s*=\s*"([^"]+)"', line):
        path = path_match.group(1)
        return path and not path.startswith("..")
    return False


def update_version(file_path, new_version):
    updated_lines = []
    in_dependencies = False

    with open(file_path, 'r') as file:
        for line in file:
            line = line.strip()
            if line == "[package]":
                in_dependencies = False
            elif line == "[dependencies]":
                in_dependencies = True
            elif line.startswith("[") and line.endswith("]"):
                in_dependencies = "dependencies" in line
            elif should_update_version(line, in_dependencies):
                line = re.sub(r'version\s*=\s*"[^"]+"', f'version = "{new_version}"', line)
            updated_lines.append(line)

    with open(file_path, 'w') as file:
        file.write("\n".join(updated_lines) + "\n")


def main():
    if len(sys.argv) != 2:
        print("Usage: python update_version.py <new_version>")
        sys.exit(1)

    new_version = sys.argv[1]
    files_to_update = [
        "Cargo.toml",
        "macro/Cargo.toml",
        "test_proj/Cargo.toml"
    ]

    for file_path in files_to_update:
        if os.path.exists(file_path):
            update_version(file_path, new_version)
            print(f"Updated version in {file_path}")
        else:
            print(f"Warning: {file_path} not found")


if __name__ == "__main__":
    main()

