# rust-alc-api 開発用 Makefile
#
# ローカル開発: make test (DB不要、ユニットのみ)
# CI はフルテスト + カバレッジリグレッションを保証

.PHONY: test test-file db-up db-down itest itest-file cov-check cov-check-unit fmt clippy

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
	source .test-config && RUST_TEST_THREADS=1 cargo test --test '*' -- --test-threads=1
	$(MAKE) db-down

# 特定テストファイル: make itest-file T=auth_test
itest-file:
	source .test-config && RUST_TEST_THREADS=1 cargo test --test $(T) -- --test-threads=1

# --- カバレッジ検証 ---

cov-check:
	source .test-config && bash scripts/check_coverage_100.sh

cov-check-unit:
	bash scripts/check_coverage_100.sh --unit-only

# --- Lint ---

fmt:
	cargo fmt --check

clippy:
	cargo clippy -- -D warnings
