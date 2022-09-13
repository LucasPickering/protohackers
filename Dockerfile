# Use a builder image to build the binary
# Keep this in sync with rust-toolchain.toml
FROM rust:1.63-slim AS builder

ENV RUSTFLAGS='-C linker=x86_64-linux-gnu-gcc'

WORKDIR /app
RUN apt-get update && \
    apt-get install -y \
    gcc-x86-64-linux-gnu \
    musl-tools \
    musl-dev \
    pkg-config \
    && \
    rm -rf /var/lib/apt/lists/*

# Install toolchain first so code changes don't force a re-run
COPY rust-toolchain.toml .
# *After* copying in files, so we have rust-toolchain.toml
RUN rustup target add x86_64-unknown-linux-musl

COPY . .
# Alpine uses musl, not glibc
RUN cargo build --release --target x86_64-unknown-linux-musl


# Copy binary into a minimal runtime image
FROM alpine:latest
WORKDIR /app
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/protohackers .
CMD ["/app/protohackers", "--host", "0.0.0.0", "1"]
