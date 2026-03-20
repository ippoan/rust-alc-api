# 認証統合計画: auth-worker → rust-alc-api に一本化

## 現状

```
┌──────────────────┐     ┌─────────────┐     ┌──────────────┐
│   auth-worker     │────→│  rust-logi   │     │ rust-alc-api  │
│  (CF Workers)     │gRPC │  AuthService │     │              │
│                   │     │              │     │ /api/auth/*   │
│ LINE WORKS OAuth  │     │ JWT発行      │     │ Google OAuth  │
│ Google OAuth      │     │ (JWT_SECRET_A)│    │ (JWT_SECRET_B)│
└───────────────────┘     └──────────────┘     └──────────────┘
        ↓ JWT (JWT_SECRET_A)                        ↓ JWT (JWT_SECRET_B)
   nuxt-pwa-carins                              alc-app
   nuxt-items                                   (デバイス: X-Tenant-ID)
   nuxt-dtako-logs
```

**問題点:**
- JWT_SECRET が2系統（rust-logi と rust-alc-api で異なる）
- nuxt-pwa-carins はプロキシで JWT.org → X-Tenant-ID に変換するワークアラウンドが必要
- auth-worker → rust-logi → JWT 発行の3段構成が冗長
- SSO 設定が alc_api スキーマに移動済みだが、参照は rust-logi の SECURITY DEFINER 関数経由

## ゴール

```
┌──────────────────────────────────────────────────┐
│                  rust-alc-api                     │
│                                                  │
│  /api/auth/google          → Google OAuth JWT    │
│  /api/auth/google/code     → Google Code Flow    │
│  /api/auth/lineworks/redirect → LW OAuth 開始    │
│  /api/auth/lineworks/callback → LW OAuth JWT     │
│  /api/auth/refresh         → Refresh Token       │
│                                                  │
│  JWT_SECRET: 統一                                 │
│  JWT: {sub, tenant_id, email, name, role}        │
└──────────────────────────────────────────────────┘
        ↓ JWT (統一 JWT_SECRET)
   全フロントエンド
```

**auth-worker 廃止、rust-logi AuthService 廃止。**

## 実装ステップ

### Step 1: rust-alc-api の LINE WORKS OAuth を本番対応

Phase 2 で実装済みだが、以下が未対応:

- [ ] WOFF SDK 対応エンドポイント (`/api/auth/woff-config`, `/api/auth/woff`)
- [ ] cross-subdomain cookie 設定（`Set-Cookie: logi_auth_token=...; Domain=.mtamaramu.com`）
- [ ] `lw_callback` パラメータ対応（OAuth コールバック戻りのマーカー）
- [ ] テスト: LINE WORKS 実環境で OAuth フロー通しテスト

**参考**: auth-worker の実装
- `workers/auth-worker/src/handlers/lineworks-redirect.ts`
- `workers/auth-worker/src/handlers/lineworks-callback.ts`

### Step 2: ユーザーテーブル統合

現状:
- `logi.app_users` + `logi.user_organizations` + `logi.oauth_accounts` （rust-logi）
- `alc_api.users` （rust-alc-api）

統合方針:
- [ ] `alc_api.users` に既存の `logi.app_users` のデータをマージ
- [ ] `lineworks_id` で LINE WORKS ユーザーを紐づけ
- [ ] `google_sub` で Google ユーザーを紐づけ
- [ ] 多組織対応が必要な場合は `user_tenants` テーブル追加（現状は 1:1）

### Step 3: JWT_SECRET 統一

- [ ] rust-alc-api の JWT_SECRET を Secret Manager `rust-logi-jwt-secret` の値に変更
  - または新しい共通 SECRET を作成して両方を更新
- [ ] JWT クレームを統一: `{sub: UUID, tenant_id: UUID, email, name, role}`
- [ ] cf-grpc-proxy の JWT_SECRET も同じ値に更新

### Step 4: フロントエンド認証先変更

#### nuxt-pwa-carins
- [ ] `server/middleware/auth.ts` — リダイレクト先を auth-worker → rust-alc-api に変更
- [ ] `plugins/auth.client.ts` — auth-worker URL → rust-alc-api URL
- [ ] `server/api/proxy/[...path].ts` — JWT.org → X-Tenant-ID 変換を削除（JWT に tenant_id が直接入る）
- [ ] WOFF SDK の woff-config エンドポイントを rust-alc-api に向ける

#### nuxt-items
- [ ] `plugins/auth.client.ts` — auth-worker → rust-alc-api
- [ ] `composables/useAuth.ts` — auth-worker → rust-alc-api

#### nuxt-dtako-logs
- [ ] `plugins/auth.client.ts` — auth-worker → rust-alc-api
- [ ] `composables/useAuth.ts` — auth-worker → rust-alc-api

#### alc-app
- [ ] 変更なし（既に rust-alc-api を使用）

### Step 5: auth-worker 廃止

- [ ] auth-worker の Cloudflare Workers を停止
- [ ] `auth.mtamaramu.com` カスタムドメインを rust-alc-api に付け替え（または新ドメイン）
- [ ] `@yhonda-ohishi-pub-dev/auth-client` パッケージの更新 or 廃止
- [ ] cf-grpc-proxy の Service Binding `GRPC_PROXY` 削除

### Step 6: rust-logi AuthService 廃止

- [ ] `src/services/auth_service.rs` 削除
- [ ] `src/services/sso_providers.rs` 削除
- [ ] `src/services/lineworks_auth.rs` 削除
- [ ] proto: `auth.proto` から認証関連 RPC 削除
- [ ] `logi.app_users`, `logi.oauth_accounts`, `logi.user_organizations` テーブルを alc_api に移動 or 廃止

## リスク

| リスク | 影響 | 対策 |
|--------|------|------|
| JWT_SECRET 切り替え時の全セッション無効化 | 全ユーザーが再ログイン | 段階的: 旧SECRET も一時的に受け入れるデュアルデコーダー |
| LINE WORKS OAuth 設定の不整合 | ログインできない | テスト環境で先行検証 |
| WOFF SDK 対応漏れ | LINE WORKS アプリ内認証が動かない | auth-worker の実装を忠実に移植 |
| auth-client npm パッケージの依存 | nuxt-items / nuxt-dtako-logs が壊れる | パッケージ更新 or 直接 composable 置換 |

## 実施順序

1. Step 1 (LW OAuth 本番対応) → テスト
2. Step 3 (JWT_SECRET 統一) → 影響範囲限定
3. Step 4 (nuxt-pwa-carins 認証先変更) → 1フロントずつ
4. Step 4 (nuxt-items, nuxt-dtako-logs)
5. Step 2 (ユーザーテーブル統合)
6. Step 5 (auth-worker 廃止)
7. Step 6 (rust-logi AuthService 廃止)
