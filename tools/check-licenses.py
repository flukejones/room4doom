#!/usr/bin/env python3
"""Reject dependencies that require a GPL-family license, including transitive crates."""

import json
from pathlib import Path
import re
import subprocess
import sys


def permits_non_gpl(expression):
    # Older Cargo manifests used '/' to express a choice of licenses.
    expression = expression.replace("/", " OR ")
    tokens = re.findall(r"[\w.+-]+|[()]", expression)
    if re.sub(r"[\w.+-]+|[()]|\s+", "", expression):
        raise ValueError("invalid SPDX expression")
    position = 0

    def term():
        nonlocal position
        token = tokens[position]
        position += 1
        if token == "(":
            allowed = choice()
            if tokens[position] != ")":
                raise ValueError("unclosed SPDX group")
            position += 1
        else:
            if token in ("AND", "OR", "WITH", ")"):
                raise ValueError("missing license identifier")
            allowed = not re.search(r"(?:^|-)A?L?GPL(?:-|$)", token)
            if position < len(tokens) and tokens[position] == "WITH":
                # An exception does not change the underlying license family.
                position += 2
                if position > len(tokens):
                    raise ValueError("missing SPDX exception")
        return allowed

    def conjunction():
        nonlocal position
        allowed = term()
        while position < len(tokens) and tokens[position] == "AND":
            position += 1
            allowed &= term()
        return allowed

    def choice():
        nonlocal position
        allowed = conjunction()
        while position < len(tokens) and tokens[position] == "OR":
            position += 1
            allowed |= conjunction()
        return allowed

    try:
        allowed = choice()
    except IndexError as error:
        raise ValueError("incomplete SPDX expression") from error
    if position != len(tokens):
        raise ValueError("unexpected SPDX token")
    return allowed


def main():
    metadata = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=Path(__file__).resolve().parent.parent,
        check=True, capture_output=True, text=True,
    )
    failures = []
    for package in json.loads(metadata.stdout)["packages"]:
        license_id = package.get("license") or ""
        try:
            allowed = permits_non_gpl(license_id)
        except ValueError as error:
            failures.append(f"{package['name']}: {error}: {license_id!r}")
            continue
        if not allowed:
            failures.append(f"{package['name']} {package['version']}: {license_id}")
    if failures:
        sys.exit("Dependency license check failed:\n" + "\n".join(failures))
    print("Dependency licenses: no GPL-family license is required")


if __name__ == "__main__":
    main()
