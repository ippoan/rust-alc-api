# rust-alc-api リファクタリング指示書: repository crate化 + バイナリ分割

## 目的

`src/db/repository/*` を各ドメインcrateに統合し、最終的にバイナリを分割することで:

1. **ローカル差分ビルドの高速化**: 修正→確認サイクルを ~150秒 → ~20秒に短縮
2. **デプロイの影響範囲縮小**: 変更したサービスだけデプロイ
3. **カバレッジ100%維持**: テスト構造は変えない
4. **将来の再利用性**: 新規バックエンド(北海大運等)から認証基盤を依存として使える

## 現状の構造

```
rust-alc-api/
├── crates/                    # ビジネスロジック (47ファイル100%カバレッジ)
│   ├── alc-core/              # 認証・JWT・ミドルウェア (880行, 98%)
│   ├── alc-carins/            # 車検証 (367行, 100%)
│   ├── alc-compare/           # 比較ロジック (3987行, 100%)
│   ├── alc-csv-parser/        # CSV解析 (963行, 100%)
│   ├── alc-devices/           # デバイス管理 (1111行, 100%)
│   ├── alc-dtako/             # デジタコ (3587行, 100%)
│   ├── alc-misc/              # 認証route・従業員等 (2186行, 100%)
│   ├── alc-pdf/               # PDF生成 (1116行, 100%)
│   ├── alc-storage/           # ストレージ R2/GCS (86行, 0%)
│   └── alc-tenko/             # 点呼系 (1690行, 100%)
├── src/                       # 本番バイナリ (1498行)
│   ├── db/repository/         # ← 848行、全ファイル0% (ここが問題)
│   │   ├── auth.rs
│   │   ├── tenko_sessions.rs
│   │   ├── devices.rs
│   │   └── ... (30+ファイル)
│   ├── main.rs                # AppState組み立て + サーバー起動
│   ├── routes/
│   ├── middleware/
│   └── bin/migrate.rs
└── tests/                     # インテグレーションテスト
```

### 今の問題

`src/db/repository/*` が全て `src/` (1つのcompilation unit) に入っているため:

- repository内の1ファイルを修正 → `src/` 全体が再コンパイル → 全crateとリンク
- キャッシュが効いていても ~190秒かかる
- CIキャッシュなしだと 8m 9s

## 目標の構造

### Phase 1: repository crate化

`src/db/repository/*` を対応するドメインcrateに `repo.rs` として移動する。

```
crates/
├── alc-core/
│   └── src/
│       ├── lib.rs
│       ├── auth_jwt.rs
│       ├── auth_google.rs
│       ├── auth_lineworks.rs
│       ├── auth_middleware.rs
│       ├── models.rs
│       ├── tenant.rs
│       └── repo.rs            ← src/db/repository/auth.rs を移動
├── alc-carins/
│   └── src/
│       ├── car_inspections.rs
│       ├── car_inspection_files.rs
│       ├── carins_files.rs
│       ├── nfc_tags.rs
│       └── repo.rs            ← src/db/repository/car_inspections.rs, carins_files.rs, nfc_tags.rs を統合
├── alc-tenko/
│   └── src/
│       ├── tenko_sessions.rs
│       ├── tenko_call.rs
│       ├── tenko_records.rs
│       ├── tenko_schedules.rs
│       ├── tenko_webhooks.rs
│       ├── equipment_failures.rs
│       ├── daily_health.rs
│       ├── health_baselines.rs
│       └── repo.rs            ← src/db/repository/tenko_*.rs, equipment_failures.rs, daily_health.rs, health_baselines.rs を統合
├── alc-dtako/
│   └── src/
│       ├── dtako_*.rs
│       └── repo.rs            ← src/db/repository/dtako_*.rs を統合
├── alc-devices/
│   └── src/
│       ├── devices.rs
│       └── repo.rs            ← src/db/repository/devices.rs を移動
├── alc-misc/
│   └── src/
│       ├── employees.rs
│       ├── measurements.rs
│       ├── timecard.rs
│       ├── carrying_items.rs
│       ├── communication_items.rs
│       ├── guidance_records.rs
│       ├── bot_admin.rs
│       ├── sso_admin.rs
│       ├── tenant_users.rs
│       ├── upload.rs
│       ├── auth.rs
│       ├── driver_info.rs
│       └── repo.rs            ← src/db/repository/ の対応ファイルを統合
└── (alc-compare, alc-csv-parser, alc-pdf, alc-storage は変更なし)
```

### repository移動のマッピング

| src/db/repository/ | 移動先 crate |
|---|---|
| auth.rs | alc-core/src/repo.rs |
| car_inspections.rs, carins_files.rs, nfc_tags.rs | alc-carins/src/repo.rs |
| tenko_sessions.rs, tenko_records.rs, tenko_schedules.rs, tenko_call.rs, tenko_webhooks.rs | alc-tenko/src/repo.rs |
| equipment_failures.rs, daily_health.rs, health_baselines.rs | alc-tenko/src/repo.rs (同上) |
| dtako_*.rs (9ファイル) | alc-dtako/src/repo.rs |
| devices.rs | alc-devices/src/repo.rs |
| employees.rs, measurements.rs, timecard.rs, carrying_items.rs, communication_items.rs, guidance_records.rs, bot_admin.rs, sso_admin.rs, tenant_users.rs, webhook.rs, driver_info.rs, dtako_upload.rs | alc-misc/src/repo.rs |

### 各 repo.rs の構造

既存の `src/db/repository/*.rs` の内容をそのまま移動する。パターンは全て同じ:

```rust
// crates/alc-tenko/src/repo.rs
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

// trait は同crate内で定義済み (alc_core::repository::* から re-export)
use crate::repository::tenko_sessions::*;

pub struct PgTenkoSessionsRepository {
    pool: PgPool,
}

impl PgTenkoSessionsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenkoSessionsRepository for PgTenkoSessionsRepository {
    // 既存の impl をそのままコピー
}
```

### Cargo.toml の変更

各crateの `Cargo.toml` に `sqlx` 依存を追加:

```toml
[dependencies]
sqlx = { version = "0.8", features = ["postgres", "runtime-tokio-native-tls", "uuid", "chrono"] }
async-trait = "0.1"
# ... 既存の依存
```

### main.rs の変更

```rust
// Before
use crate::db::repository::tenko_sessions::PgTenkoSessionsRepository;
use crate::db::repository::auth::PgAuthRepository;

// After
use alc_tenko::repo::PgTenkoSessionsRepository;
use alc_core::repo::PgAuthRepository;
```

AppState の組み立ては import パスが変わるだけで、ロジックは変わらない。

## Phase 2: バイナリ分割

Phase 1 完了後、`src/main.rs` を4つのバイナリに分割する。

### バイナリ構成

| バイナリ | 依存crate | 行数(概算) | 用途 |
|---|---|---|---|
| alc-api | alc-core, alc-misc, alc-storage, alc-devices | ~4,263行 | 認証・従業員・アルコールチェック・デバイス |
| carins-api | alc-core, alc-carins, alc-storage | ~1,333行 | 車検証管理 |
| tenko-api | alc-core, alc-tenko | ~2,570行 | 点呼系全部 |
| dtako-api | alc-core, alc-dtako, alc-csv-parser, alc-pdf, alc-compare | ~10,533行 | デジタコ・拘束時間レポート |

### ワークスペース構成

```toml
# Cargo.toml (workspace root)
[workspace]
members = [
    "crates/*",
    "services/alc-api",
    "services/carins-api",
    "services/tenko-api",
    "services/dtako-api",
]
```

```
services/
├── alc-api/
│   ├── Cargo.toml
│   └── src/main.rs      # alc-core + alc-misc + alc-storage + alc-devices
├── carins-api/
│   ├── Cargo.toml
│   └── src/main.rs      # alc-core + alc-carins + alc-storage
├── tenko-api/
│   ├── Cargo.toml
│   └── src/main.rs      # alc-core + alc-tenko
└── dtako-api/
    ├── Cargo.toml
    └── src/main.rs       # alc-core + alc-dtako + alc-csv-parser + alc-pdf + alc-compare
```

### 各 service の main.rs パターン

```rust
// services/tenko-api/src/main.rs
use alc_core::{create_app_state, auth_routes};
use alc_tenko::{tenko_routes, repo::PgTenkoSessionsRepository};

#[tokio::main]
async fn main() {
    let pool = /* DB接続 */;
    let tenko_repo = PgTenkoSessionsRepository::new(pool.clone());

    let app = Router::new()
        .nest("/api/auth", auth_routes())
        .nest("/api", tenko_routes())
        .layer(/* ミドルウェア */);

    // サーバー起動
}
```

### Cloud Run 構成

- 各バイナリを別の Cloud Run サービスとしてデプロイ
- `min-instances=0` でリクエストなし時のコスト $0
- Dockerfile をマルチステージビルドで共有、`--bin` 引数でバイナリ選択
- deploy.sh を各サービス対応に拡張

### CI の変更

```yaml
# .github/workflows/ci.yml
# テストは変わらない (workspace全体でcargo test)
# ビルドを並列化
jobs:
  build:
    strategy:
      matrix:
        service: [alc-api, carins-api, tenko-api, dtako-api]
    steps:
      - run: cargo build --release --bin ${{ matrix.service }}
```

## 実行手順

### Phase 1: repository crate化 (推奨: 先にやる)

1. **ブランチ作成**: `fix/repo-crate-migration`
2. **各crateに `repo.rs` を追加**: `src/db/repository/*.rs` の内容を対応crateに移動
3. **各crateの `Cargo.toml` 更新**: sqlx 依存追加
4. **各crateの `lib.rs` 更新**: `pub mod repo;` 追加
5. **`src/main.rs` のimport修正**: `crate::db::repository::*` → `alc_*::repo::*`
6. **`src/db/repository/` 削除**
7. **`cargo fmt` + `cargo clippy`**
8. **CI確認**: 全テスト通過 + カバレッジ100%維持を確認
9. **PR → merge**

### Phase 2: バイナリ分割 (Phase 1完了後)

1. **ブランチ作成**: `fix/binary-split`
2. **`services/` ディレクトリ作成**: 4つのバイナリ用
3. **各 `services/*/src/main.rs` 作成**: 今の `src/main.rs` からルート振り分けを分割
4. **`src/main.rs` 削除** (または全サービスを含むモノリスとして残す選択肢もある)
5. **Dockerfile 修正**: マルチステージビルド + `--bin` 引数
6. **`deploy.sh` 修正**: サービス選択対応
7. **CI修正**: matrix ビルド
8. **Cloud Run サービス作成**: 各バイナリ用
9. **テスト + カバレッジ確認**
10. **PR → merge**

## 効果見積もり

| 項目 | 今 | Phase 1後 | Phase 2後 |
|---|---|---|---|
| ローカル差分ビルド | ~150秒 | ~60秒 | ~20秒 |
| ローカルテスト実行 | ~60秒 | ~30秒 | ~10秒 |
| 修正→確認サイクル | ~210秒 | ~90秒 | ~30秒 |
| CI (キャッシュあり) | ~179秒 | ~120秒 | ~30秒 |
| CI (キャッシュなし) | ~489秒 | ~400秒 | ~350秒 |
| デプロイ影響範囲 | 全サービス | 全サービス | 変更分のみ |
| カバレッジ | 47ファイル100% | 維持 | 維持 |

## 注意事項

- **Phase 1 だけでも価値がある**: repository がcrateに入ることでsrc/が軽くなり、差分ビルドが改善する
- **テスト構造は変えない**: 既存のmockテスト、インテグレーションテストはそのまま
- **RLSの検証方式は変えない**: splinter (Supabase Postgres Linter) での検証を継続
- **適用済みマイグレーションは変更しない**: SQLxのSHA-384チェックサムが不一致になる
- **`src/db/repository/*.rs` のパターン**: 全ファイルが `PgPool` を持つ struct + `#[async_trait] impl Trait` の形式。移動時に構造変更は不要
- **`alc-storage` (R2/GCS) はカバレッジ0%のまま**: 本番のみで動作するため変更なし
- **coverage_100.toml の更新**: repository が各crateに移動した場合、パスが変わるのでファイル一覧を更新する
