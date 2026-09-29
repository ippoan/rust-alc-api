# staging の alc-vein Worker が繋ぐ DB (Cloudflare Containers、Refs #691 / #680)。
# postgres + PgBouncer (transaction mode) を 1 つの Container に入れる。
#
# 本番の経路 (Supabase のプーラー 6543、transaction mode) と同じく「トランザクションごとに
# サーバー側のコネクションが使い回される」条件を作るため、Worker は postgres ではなく
# PgBouncer (6432) にだけ繋ぐ。Container へ届くのは Durable Object (`VeinDb`) の
# getTcpPort() 経由だけで、外からは届かない。
#
# ディスクは揮発 (sleep 後の再起動で image の状態に戻る) ので、毎回の起動で空の DB から
# init_local_db.sql → migrations → local_app_grants.sql (本番の GRANT の写し、#689) を流す
# (staging/entrypoint.sh と同じ方式)。build context は repo のルート (wrangler.toml の
# image_build_context)。持ち込むファイルは Dockerfile.dockerignore で絞る。
FROM postgres:16

RUN apt-get update \
    && apt-get install -y --no-install-recommends pgbouncer \
    && rm -rf /var/lib/apt/lists/*

# 認証は trust (staging/Dockerfile.postgres と同じ)。postgres も PgBouncer も Container の
# 外へは出ず、到達経路は VeinDb の getTcpPort() だけなので、パスワードを持たない。
ENV POSTGRES_HOST_AUTH_METHOD=trust
# VOLUME (/var/lib/postgresql/data) に頼らず、揮発のディスク上の普通のディレクトリに置く
ENV PGDATA=/tmp/vein-pgdata

COPY scripts/init_local_db.sql /vein/init_local_db.sql
COPY scripts/local_app_grants.sql /vein/local_app_grants.sql
COPY migrations /vein/migrations
COPY workers/vein/tests/seed.sql /vein/seed.sql
COPY workers/vein/container/pgbouncer.ini /etc/pgbouncer/pgbouncer.ini
COPY workers/vein/container/start.sh /vein/start.sh
RUN chmod +x /vein/start.sh && chown -R postgres:postgres /vein /etc/pgbouncer

# start.sh が root なら gosu で postgres に降りる (どちらのユーザーで起動されても動く)
USER postgres
EXPOSE 6432
CMD ["/vein/start.sh"]
