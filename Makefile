# rust-alc-api 開発用 Makefile
#
# ローカル開発: make test (DB不要、ユニットのみ)
# CI はフルテスト + カバレッジリグレッションを保証

.PHONY: test test-file db-up db-down itest itest-file cov-check cov-check-unit fmt clippy cov-dl cov-summary cov-not100 cov-file

# --- ユニットテスト (DB 不要、高速) ---

test:
	cargo test --lib

# 特定モジュールのテスト: make test-file F=jwt
test-file:
	cargo test --lib $(F)

# --- DB 管理 ---

db-up:
	docker compose up -d test-db
	@echo "Waiting for PostgreSQL..."
	@until pg_isready -h localhost -p 54322 -q 2>/dev/null; do sleep 1; done
	@echo "PostgreSQL ready."

db-down:
	docker compose down

# --- インテグレーションテスト (DB 必要) ---

itest: db-up
	source .test-config && cargo test --test '*'
	$(MAKE) db-down

# 特定テストファイル: make itest-file T=auth_test
itest-file:
	source .test-config && cargo test --test $(T)

# --- カバレッジ検証 ---

cov-check:
	source .test-config && bash scripts/check_coverage_100.sh

cov-check-unit:
	bash scripts/check_coverage_100.sh --unit-only

cov-check-mock:
	bash scripts/check_coverage_100.sh --mock-only

# --- Mock テスト (DB 不要) ---

mock-test:
	cargo test --test 'mock_*'

mock-cov:
	cargo llvm-cov --test 'mock_*' --summary-only

# --- CI カバレッジ取得 (artifact からダウンロード) ---

COV_CACHE := /tmp/llvm-cov-cache/ci-latest.txt

# 最新 CI run から artifact ダウンロード
cov-dl:
	@mkdir -p /tmp/llvm-cov-cache
	gh run download --name llvm-cov-text --dir /tmp/llvm-cov-dl 2>/dev/null || \
		(echo "Artifact not found. Trying latest successful integration-tests run..." && \
		 gh run download $$(gh run list --workflow ci.yml --status success --json databaseId -q '.[0].databaseId') \
		   --name llvm-cov-text --dir /tmp/llvm-cov-dl)
	mv /tmp/llvm-cov-dl/llvm-cov-output.txt $(COV_CACHE)
	rm -rf /tmp/llvm-cov-dl
	@echo "Downloaded to $(COV_CACHE)"

# CI カバレッジ → サマリ
cov-summary: cov-dl
	bash scripts/parse_coverage.sh summary "" $(COV_CACHE)

# CI カバレッジ → 未達成ファイル一覧
cov-not100: cov-dl
	bash scripts/parse_coverage.sh not-100 "" $(COV_CACHE)

# CI カバレッジ → 特定ファイルの未カバー行: make cov-file F=devices
cov-file: cov-dl
	bash scripts/parse_coverage.sh file $(F) $(COV_CACHE)

# --- Lint ---

fmt:
	cargo fmt --check

clippy:
	cargo clippy -- -D warnings
