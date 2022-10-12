# Actual version doesn't matter here since we use a nightly one
FROM rust:1.63-slim

WORKDIR /app/api
COPY rust-toolchain.toml .
# Force rustup to install the toolchain
RUN rustup show
COPY . .
ENTRYPOINT ["cargo", "run", "--"]
