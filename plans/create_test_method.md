# 目的
## 現状　
backend を deploy 時に　migrateして、front　end もdeployしているが、このままでは、migration ,backend frontend 更新時に時間差が生まれてエラーが起こる可能性がある。

さらにbackendの修正が必要な場合であれば、gcpによるroutingはあるが、backend error のまま、
修正終わるまで放置になる

## とるべき対策
 - migration のテストをlocalで行う
 - migration ｔｅｓｔ時に　supabase のlinterを使ってエラー検知する
 - backend testをlocal dbで行う
 - fronted testをvite, wrangler, playwrightで行う
 - 一括deployのshを作成