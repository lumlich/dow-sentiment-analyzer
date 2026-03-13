FROM node:20-bookworm-slim AS ui-builder
WORKDIR /app/ui.off

COPY ui.off/package*.json ./
RUN if [ -f package-lock.json ]; then npm ci; else npm install; fi

COPY ui.off/ ./
RUN npm run build


FROM rust:1-bookworm AS rust-builder
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY sentiment_lexicon.json ./
COPY tests/fixtures ./tests/fixtures
COPY assets ./assets

COPY --from=ui-builder /app/ui.off/dist ./assets

RUN cargo build --release --bin server


FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=rust-builder /app/target/release/server /app/server
COPY --from=rust-builder /app/assets /app/assets
COPY config ./config

RUN mkdir -p /app/data

ENV PORT=8000

EXPOSE 8000

CMD ["/app/server"]
