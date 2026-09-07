#!/bin/bash
################################################################################
#
#    Copyright (c) 2026 Haixing Hu.
#
#    SPDX-License-Identifier: Apache-2.0
#
################################################################################

set -euo pipefail

cargo test --locked --all-features \
  --manifest-path fixtures/s3-contract/Cargo.toml
