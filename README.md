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

## private な git 依存 (vein-match)

`crates/alc-vein` は指静脈照合の `vein-match-search` を **private repo `ippoan/vein-match`** から
git 依存 (tag 固定) で取り込んでいる。取得に GitHub の認証が要る:

- **ローカル**: `gh auth login` のうえで `gh auth setup-git` 済みであること (cargo も Bazel も
  git の credential helper 経由で取る)。確認は `git ls-remote https://github.com/ippoan/vein-match`
- **CI**: cargo / bazel を打つ job は checkout 直後に `ippoan/ci-workflows/.github/actions/private-git-auth`
  を呼ぶ (GitHub App `ippoan-ci-bot` の token で git の URL を `url.<token 付き URL>.insteadOf` で
  書き換える。cargo も Bazel の crate_universe もこれを読む)。ci-workflows の reusable のうち
  workspace を解決するもの (`rust-dep-check.yml` / `catalog-extract.yml`) には
  `private_git_repos: vein-match` と secrets `CI_APP_ID` / `CI_APP_PRIVATE_KEY` を渡す。
  **cargo / bazel を打つ job を足すときはこの step も足す**。fork / Dependabot の PR は secrets が
  無いので取得できない

tag を上げるときは `crates/alc-vein/Cargo.toml` の `tag` を変えて `cargo update -p vein-match-search`
で `Cargo.lock` も更新する。

## pre-commit hook (fmt / clippy)

`.githooks/pre-commit` が commit 前に `cargo fmt --check` と `cargo clippy --workspace --all-targets -- -D warnings` を走らせる。

### 初回 setup (clone 直後 1 度だけ)

```bash
git config core.hooksPath .githooks
```

clippy が遅い (~30s+) ときは `SKIP_CLIPPY=1 git commit ...` で一時 skip 可 (CI では必ず走る)。

> 旧 plan/snapshot 整合性チェック (`ippoan/ippoan-dev-plans` ↔ `manifests/production.snapshot.json`) は
> dev-plans repo の archive に伴い撤去した。`if_flag!()` の消費者は無かったため code への影響は無い。
