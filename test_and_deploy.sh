#!/bin/bash
# rust-alc-api: 共通テスト+デプロイスクリプトを呼び出す
exec bash ~/.claude/skills/migrate-test/scripts/test_and_deploy.sh "$@"
