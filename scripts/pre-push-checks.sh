#!/bin/bash
# git push 前にローカルで走らせる repo 固有チェック。
# claude-hooks の pre-push-repo-checks.sh (PreToolUse hook) が git push の
# たびに自動実行する (手動実行も可)。追加するチェックは「数秒以内・
# ネットワーク不要」のものに限ること (push のたびに毎回走るため)。
set -euo pipefail
cd "$(dirname "$0")/.."

# bazel test matrix / warm 同期 / coverage config / dev-deps 配線 (Refs #515 / #539)
bash scripts/check_bazel_test_matrix.sh
