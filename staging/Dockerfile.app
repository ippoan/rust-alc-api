FROM debian:trixie-slim

RUN apt-get update && apt-get install -y ca-certificates postgresql-client && rm -rf /var/lib/apt/lists/*

COPY rust-alc-api /usr/local/bin/
COPY migrate /usr/local/bin/
COPY archive /usr/local/bin/
COPY migrations /app/migrations
COPY staging/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

WORKDIR /app
ENV PORT=8080
EXPOSE 8080

CMD ["/app/entrypoint.sh"]
