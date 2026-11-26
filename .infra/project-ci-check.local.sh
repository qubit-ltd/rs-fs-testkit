#!/usr/bin/env bash
set -euo pipefail

project_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$project_root"

python3 -B scripts/check-doc-links.py
python3 -B -m unittest discover -s scripts/tests
