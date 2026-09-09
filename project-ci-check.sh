#!/bin/bash
################################################################################
#
#    Copyright (c) 2026 Haixing Hu.
#
#    SPDX-License-Identifier: Apache-2.0
#
################################################################################

set -euo pipefail

python3 -m unittest discover -s scripts/tests -p test_check_fs_ecosystem.py

cargo test --locked --all-features \
  --manifest-path fixtures/s3-contract/Cargo.toml
