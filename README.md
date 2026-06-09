# rust-alc-api

アルコールチェッカーシステムのバックエンド API。GCP Cloud Run にデプロイ。

**別リポジトリで管理**

## 技術スタック

- Rust (Axum)
- GCP Cloud Run
- PostgreSQL + RLS (Row Level Security)
- GCP Cloud Storage (顔写真)

## 主な機能

- 測定結果の CRUD API
- 乗務員管理 API
- 顔写真アップロード (Cloud Storage)
- RLS によるマルチテナントデータ分離

## pre-commit hook (fmt / clippy)

`.githooks/pre-commit` が commit 前に `cargo fmt --check` と `cargo clippy --workspace --all-targets -- -D warnings` を走らせる。

### 初回 setup (clone 直後 1 度だけ)

```bash
git config core.hooksPath .githooks
```

clippy が遅い (~30s+) ときは `SKIP_CLIPPY=1 git commit ...` で一時 skip 可 (CI では必ず走る)。

> 旧 plan/snapshot 整合性チェック (`ippoan/ippoan-dev-plans` ↔ `manifests/production.snapshot.json`) は
> dev-plans repo の archive に伴い撤去した。`if_flag!()` の消費者は無かったため code への影響は無い。
