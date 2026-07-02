# kiosk 端末 re-pair (再認証) 設計

Refs #495 (tracking hub) / #480 / #434

本 doc は #495 の PR1 (rust-alc-api) の詳細設計 SoT。以降の実装 PR (本 PR 以降) は
ここに書かれた schema / endpoint 契約 / 判定ロジックを実装として落とし込む。

## 背景

lockdown (#434) 後、kiosk 端末は auth-worker が発行する device credential
(`auth_device_id` + `device_secret`) から device JWT を mint し、認証必須 API
(`/api/devices/settings/*` 系、`register-fcm-token`、`report-*`) を通す。

既存登録済み端末の一部はこの credential を持たない (過去の登録経路が funnel を
通らず取りこぼしていた。新規登録の取りこぼしは ippoan/alc-app#81 / #83 で修正済み)。
これらの端末を **フル再登録せずに credential だけ再取得させる** のが re-pair。

## 設計方針 (確定、Fable 相談で決定)

- **mint 経路は 1 本**: 実際に auth-worker `/device/pair-internal` を叩いて
  `device_secret` を新規発行するのは常に「端末からのリクエストを受けた
  rust-alc-api」。管理者側は mint を代行しない。
- **管理者側は認可のみ**: 管理者操作は「時限 window を開ける」だけで、
  device_secret を生成・保持・表示しない。
- **rust DB に平文 secret を置かない**: `devices` テーブルに保存するのは
  window の期限・監査カウンタ・hardware fingerprint のハッシュのみ。
  `device_secret` そのものは auth-worker の応答をそのまま端末に転送し、
  rust 側では保持しない (claim フローの `settings_token` 発行と同様、
  秘密は生成元から消費者に直行させる)。
- **失敗理由を端末に開示しない**: window 外 / status 不正 / hardware 不一致 /
  存在しない device_id、すべて **404** で返す。詳細は `tracing::error!` /
  `tracing::warn!` でサーバログにのみ出す (値は出さない)。

## シーケンス

```
[管理者]                          [rust-alc-api]                    [端末]
  │ POST /devices/{id}/authorize-repair (tenant 認証)                  │
  │───────────────────────────────▶│                                  │
  │   devices.re_pair_authorized_until = now() + 15min                │
  │   (reset_binding=true なら hardware_id も同時に NULL clear)        │
  │◀─── 200 { authorized_until } ───│                                  │
  │                                 │                                  │
  │                                 │◀── POST /api/devices/re-pair ───│
  │                                 │    { device_id, hardware_id? }  │
  │                                 │                                  │
  │                                 │ 1. status=='active' か         │
  │                                 │ 2. re_pair_authorized_until      │
  │                                 │    が未来か (admin window 判定)  │
  │                                 │ 3. cooldown (last_re_pair_at)    │
  │                                 │ 4. TOFU hardware_id 一致判定     │
  │                                 │    (全部 pass しないと 404)      │
  │                                 │                                  │
  │                                 │──POST /device/pair-internal────▶│ auth-worker
  │                                 │  (INTERNAL_SHARED_SECRET)        │
  │                                 │◀── { auth_device_id,             │
  │                                 │      device_secret } ───────────│
  │                                 │                                  │
  │                                 │  window を消費 (single-use)      │
  │                                 │  re_pair_count += 1              │
  │                                 │  last_re_pair_at = now()         │
  │                                 │  hardware_id を bind (初回のみ)  │
  │                                 │                                  │
  │                                 │──── 200 { auth_device_id,       │
  │                                 │        device_secret } ─────────▶│
```

## DB スキーマ変更 (`migrations/122_devices_re_pair.sql` 想定)

`alc_api.devices` に列追加 (すべて nullable / デフォルト値ありで既存行に影響しない):

| 列 | 型 | 意味 |
|---|---|---|
| `re_pair_authorized_until` | `timestamptz NULL` | 管理者が開けた時限 window の終了時刻。過去 or NULL なら window 外 |
| `last_re_pair_at` | `timestamptz NULL` | 直近の re-pair 成功時刻 (cooldown 判定 + 監査表示用) |
| `re_pair_count` | `integer NOT NULL DEFAULT 0` | re-pair 成功回数 (監査用、値は増分のみ) |
| `hardware_id` | `text NULL` | TOFU bind 用ハッシュ (`SHA-256(ANDROID_ID)` 等の hex。生値は保存しない) |

いずれも `X-Tenant-ID` / `tenant_id` スコープの RLS ポリシー配下（既存 `devices`
テーブルの RLS を継承、新規ポリシー追加は不要 — 列追加のみ）。

`claim_registration` の request body にも `hardware_id: Option<String>` を追加
(PR3a 側で端末が初回登録時にも送れるようにする。無ければ既存動作のまま NULL)。

## Endpoint 契約

### 管理者向け (tenant_router, JWT 認証必須)

```
POST /devices/{id}/authorize-repair
```

Request body:

```jsonc
{
  "reset_binding": false   // true なら hardware_id を同時に NULL clear (TOFU 再bind許可)
}
```

Response `200`:

```jsonc
{ "authorized_until": "2026-07-02T12:15:00Z" }
```

- 対象 device が存在しない / 他 tenant / status != `active` の場合は `404`
  (既存 `approve_device` 等と同じ tenant scoping パターンを踏襲)
- window 長は環境変数 `RE_PAIR_WINDOW_SECS` (デフォルト `900` = 15分)

### 端末向け (public_router, 認証なし — window が事実上の認可)

```
POST /devices/re-pair
```

Request body:

```jsonc
{
  "device_id": "uuid",
  "hardware_id": "sha256-hex-optional"
}
```

Response `200` (成功時のみ):

```jsonc
{ "auth_device_id": "...", "device_secret": "..." }
```

失敗時は理由を問わず `404`。ただし cooldown 中は `429` (issue の完了条件に明記
されている唯一の例外 — window/status/TOFU 不一致と cooldown は呼び出し元が
取りうる対処が違う [今は待て] ので区別してよい、と Fable 相談で確認済み)。

**window 消費はアトミック (compare-and-swap)**: 「read → 判定 → mint →
`record_re_pair_success`」の間に並行リクエストが割り込むと、両方が判定を
pass して両方に credential が発行されうる (TOCTOU race)。これを防ぐため
`record_re_pair_success` は判定時に読んだ `re_pair_authorized_until` を
`WHERE ... AND re_pair_authorized_until IS NOT DISTINCT FROM $expected` で
突合する compare-and-swap として実装する。0 行更新 (= 他リクエストが先に
消費 / 別 authorize-repair が書き換え済み) なら `Ok(false)` を返し、
呼び出し元は mint 済み credential を返さず 404 にする (Refs #495 PR1
review, C-1)。

**settings_token の「成功時 rotate」は本 PR では未実装** (S-1)。
`RE_PAIR_REQUIRE_TOKEN=true` にしても現状は「未提示は許可・提示時は一致
必須」までで、rotate は後続 PR で settings API の response 拡張と合わせて
実装する。

判定順序 (pure fn `evaluate_re_pair_request` に切り出し、DB access 無しで
unit test する):

1. `status == "active"` でなければ `Deny::NotFound`
2. `RE_PAIR_REQUIRE_ADMIN=true` (デフォルト true) の場合、
   `re_pair_authorized_until` が `now` より未来でなければ `Deny::NotFound`
3. `last_re_pair_at` が `now - RE_PAIR_COOLDOWN_SECS` (デフォルト `600`) より
   新しければ `Deny::TooManyRequests`
4. `hardware_id` が DB 側に既に bind 済みで、リクエストの `hardware_id` と
   不一致なら `Deny::NotFound` (TOFU)。DB 側が未 bind (`NULL`) ならこの
   リクエストの値で bind して通す
5. `RE_PAIR_REQUIRE_TOKEN=true` の場合、`settings_token` が request に
   含まれていれば一致必須 (未提示は許可、成功時に rotate — ratchet 方式)。
   デフォルトは `false` (段階導入、Hardening 節参照)

全て pass したら auth-worker `/device/pair-internal` を叩く。

### auth-worker 呼び出し (rust-alc-api → auth-worker、サーバー間)

```
POST {AUTH_WORKER_URL}/device/pair-internal
Authorization: Bearer <INTERNAL_SHARED_SECRET> 相当のヘッダ (auth-worker#298/#341 の
                          既存 internal 認証パターンに合わせる。ヘッダ名は
                          auth-worker 側の実装 (#298) に追従)
```

Request:

```jsonc
{ "tenant_id": "...", "label": "alc-app:<device_id>", "role": "device-alc-kiosk" }
```

- `label` は device 単位で一意にし、auth-worker 側 PR2 の `replace_label`
  (同 label の旧 credential を revoke してから新規 mint) を効かせて
  credential rotate を実現する
- 呼び出し失敗 (auth-worker 側 5xx / timeout) は rust 側も `502` 相当だが、
  端末には他の失敗と区別させず **404** に丸める (info leak 防止方針を貫く)。
  詳細は log にのみ出す

新規環境変数 (rust-alc-api 側):

| 変数 | 用途 |
|---|---|
| `AUTH_WORKER_URL` | 既存 gateway crate と同名 (rust-alc-api 本体では新規)。auth-worker の base URL |
| `RE_PAIR_INTERNAL_SHARED_SECRET` | pair-internal 呼び出し用の shared secret (auth-worker 側で allowlist する専用 secret。既存 `INTERNAL_SHARED_SECRET` の再利用ではなく新規 secret を発行する — 用途混在を避ける) |
| `RE_PAIR_REQUIRE_ADMIN` | デフォルト `true`。`false` にすると window 判定をスキップ (段階導入用、本番では常に true 運用) |
| `RE_PAIR_REQUIRE_TOKEN` | デフォルト `false`。`true` で settings_token 2-factor 化 |
| `RE_PAIR_WINDOW_SECS` | デフォルト `900` |
| `RE_PAIR_COOLDOWN_SECS` | デフォルト `600` |

## Hardening (段階導入、issue 本文の再掲 + 実装への対応)

| 対策 | 実装 |
|---|---|
| admin window (主防御) | `RE_PAIR_REQUIRE_ADMIN` (デフォルト on) |
| TOFU hardware bind | `devices.hardware_id`、判定ロジック手順4 |
| status 厳格化 | 判定ロジック手順1 (active のみ、devices テーブルの稼働状態) |
| credential rotate | auth-worker PR2 `replace_label` (rust 側は label を渡すだけ) |
| settings_token co-factor | `RE_PAIR_REQUIRE_TOKEN` (デフォルト off、ratchet) |
| cooldown | `devices.last_re_pair_at` + `RE_PAIR_COOLDOWN_SECS`。Cloudflare WAF rate rule は別途 alc-app 側 (インフラ設定、PR 外) |
| audit | `re_pair_count` / `last_re_pair_at` 更新 + `tracing::info!("device re-pair granted device_id={id} tenant_id={tenant}")` (値を含まない) + response に `Cache-Control: no-store` |

## テスト方針

- `evaluate_re_pair_request` (pure fn、DB 非依存) の unit test で以下を網羅:
  - active 以外は deny
  - window 外 (NULL / 過去) は deny、window 内は許可
  - cooldown 内は `TooManyRequests`
  - hardware_id 未 bind → 初回リクエストの値で bind して許可
  - hardware_id bind 済み → 不一致は deny、一致は許可
  - `RE_PAIR_REQUIRE_ADMIN=false` で window 判定をスキップ
  - `RE_PAIR_REQUIRE_TOKEN=true` で settings_token 不一致は deny、未提示は許可
- integration test (`tests/devices_re_pair_test.rs`、既存 `tests/devices_test.rs`
  のヘルパーを再利用):
  - `authorize-repair` → window 付与 → `re-pair` 成功 → 2 回目は 404 (single-use)
  - window 外 / status!=active / 存在しない device_id は全て 404
  - auth-worker 呼び出しは wiremock でモック (`crates/alc-notify` の LINE
    client と同じパターン — `with_endpoints` で差し替え可能にする)
- DB エラー注入は既存 `RENAME` パターン (SELECT 系) / trigger パターン
  (UPDATE 系) を流用

## 完了条件との対応

issue #495 の Acceptance Criteria はそのまま本 doc のテスト方針でカバーする。
`RE_PAIR_REQUIRE_ADMIN=true` で window 無し re-pair が 404 になる件、cooldown
429、TOFU 404 は上記 unit test 一覧に含まれる。

## 未確定・後続 PR で決める事項

- auth-worker `/device/pair-internal` の実リクエスト/レスポンス schema
  (PR2 側实装待ち、本 doc の記述は暫定)。確定次第 `crates/alc-devices` の
  HTTP client 実装をそれに合わせる
- `role: "device-alc-kiosk"` という role 名は仮称。auth-worker 側の既存
  role 一覧 (`device-dtako-ingest` 等) と衝突しないか PR2 実装時に確認する
- Cloudflare WAF rate rule の具体的なルール定義はインフラ設定側 (本 repo
  scope 外)
