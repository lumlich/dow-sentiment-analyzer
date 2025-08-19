# Dockerfile (put this file in your project root)
# --- Stage 1: build the app ---
FROM rust:1.80 as builder
WORKDIR /app
COPY . .

# (Optional) Speed up builds by caching dependencies:
# RUN cargo fetch

# You can set your binary name via build-arg; default is 'mvp' since your repo is named that way.
ARG BIN_NAME=mvp

# Build the release binary (change --bin if your crate has multiple binaries)
RUN cargo build --release --bin ${BIN_NAME}

# --- Stage 2: runtime image ---
FROM debian:stable-slim
WORKDIR /app

# Copy the compiled binary
ARG BIN_NAME=mvp
COPY --from=builder /app/target/release/${BIN_NAME} /usr/local/bin/app

# (Optional) If your app needs runtime configs, uncomment and copy them:
# COPY source_weights.json sentiment_lexicon.json ./

EXPOSE 8000
CMD ["app"]
