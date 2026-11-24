#!/usr/bin/env python3
"""Retain exact engine bytes without executing them (local POSIX filesystems).

Identity is SHA-256, matching SqxLabs research archives. This is byte retention,
not build provenance, a portable runtime bundle or authorization to run a binary.
"""
import argparse
import hashlib
import os
from pathlib import Path
import tempfile


def retain(executable, destination, expected=None):
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    fd, pending = tempfile.mkstemp(prefix=".pending-", dir=destination)
    try:
        digest = hashlib.sha256()
        with os.fdopen(fd, "wb") as output, open(executable, "rb") as source:
            while chunk := source.read(1024 * 1024):
                digest.update(chunk)
                output.write(chunk)
            output.flush()
            os.fsync(output.fileno())
        identity = digest.hexdigest()
        if expected is not None and identity != expected:
            raise ValueError("executable does not match the required research engine identity")
        target = destination / (identity + ".bin")
        # A retained binary deliberately has no execute permission. Publication
        # cannot overwrite another process's object or repair corruption silently.
        try:
            os.link(pending, target)
        except FileExistsError:
            if target.is_symlink() or not target.is_file():
                raise ValueError("retained engine is not a regular file")
            with target.open("rb") as existing:
                check = hashlib.sha256()
                while chunk := existing.read(1024 * 1024):
                    check.update(chunk)
                actual = check.hexdigest()
            if actual != identity:
                raise ValueError("retained engine integrity failure")
        directory = os.open(destination, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
        return target
    finally:
        os.unlink(pending)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--expected-engine-id", help="Require the hash from an existing research record")
    args = parser.parse_args()
    try:
        print(retain(args.executable, args.out, args.expected_engine_id))
    except (OSError, ValueError) as error:
        parser.exit(1, str(error) + "\n")
