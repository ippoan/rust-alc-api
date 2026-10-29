この repo は撤収して archive しました。API は Cloudflare Workers の分割 worker へ移っています。Refs ippoan/rust-alc-api#680

# rust-alc-api (撤収済み)

アルコールチェック (ALC) API の monolith (Axum + PostgreSQL RLS) でした。ドメインごとに別 repo の Cloudflare Worker へ分割し終えたため、ここには何も残っていません。

## 移り先

| ドメイン・役割 | repo |
|---|---|
| 認証と振り分け (JWT 検証、Service Binding での各 worker への中継) | [ippoan/auth-worker](https://github.com/ippoan/auth-worker) |
| ログインの内部口・機械用資格情報 | [ippoan/alc-auth-worker](https://github.com/ippoan/alc-auth-worker) |
| 社員・タイムカード | [ippoan/alc-people-worker](https://github.com/ippoan/alc-people-worker) |
| 端末登録・管理 | [ippoan/alc-devices-worker](https://github.com/ippoan/alc-devices-worker) |
| 点呼 | [ippoan/alc-tenko-worker](https://github.com/ippoan/alc-tenko-worker) |
| 測定 | [ippoan/alc-measurements-worker](https://github.com/ippoan/alc-measurements-worker) |
| 物品・携行品・伝達事項・指導記録 | [ippoan/alc-records-worker](https://github.com/ippoan/alc-records-worker) |
| 車検証・車両整備 | [ippoan/alc-carins-worker](https://github.com/ippoan/alc-carins-worker) |
| デジタコ (運行 CSV) | [ippoan/alc-dtako-worker](https://github.com/ippoan/alc-dtako-worker) |
| LINE WORKS 送信 | [ippoan/alc-lineworks-worker](https://github.com/ippoan/alc-lineworks-worker) |
| 通知の配信・宛先・文書 | [ippoan/alc-notify-worker](https://github.com/ippoan/alc-notify-worker) |
| 障害チケット (trouble) | [ippoan/alc-trouble-worker](https://github.com/ippoan/alc-trouble-worker) |
| 指静脈 (vein) | [ippoan/alc-vein-worker](https://github.com/ippoan/alc-vein-worker) |
| PDF の黒塗り・抽出 | [ippoan/alc-redact](https://github.com/ippoan/alc-redact) |
| migration (正本) | [ippoan/alc-migrations](https://github.com/ippoan/alc-migrations) |
| worker 共通の部品 (DB 接続など) | [ippoan/alc-worker-kit](https://github.com/ippoan/alc-worker-kit) |

## 撤収の要点

- Cloud Run の service (本番・staging) と archive Job は 2026-10-29 に削除した。
- dtakologs の archive は alc-dtako-worker の Cron が担う。
- GitHub の workflow はすべて無効化済み。
- 旧 README と実装は git 履歴に残っている。
