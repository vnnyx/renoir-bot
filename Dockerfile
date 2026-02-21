FROM rust:1.88.0-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libopus-dev libssl-dev cmake g++ \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./

RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

COPY src ./src

RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libopus0 libssl3 python3 pip curl unzip \
    && pip install --no-cache-dir --break-system-packages yt-dlp \
    && apt-get purge -y pip && apt-get autoremove -y \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://deno.land/install.sh | DENO_INSTALL=/usr/local sh

COPY --from=builder /app/target/release/renoir-bot /usr/local/bin/

RUN useradd -r -s /usr/sbin/nologin bot
USER bot

ENTRYPOINT ["renoir-bot"]
