# ---- build ----
FROM rust:1-bookworm AS build

WORKDIR /app

# System deps (safe for sqlx + tls)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace
COPY . .

# If you use sqlx::query! macros, this avoids needing a DB at build-time
ENV SQLX_OFFLINE=true

# Build worker + pgflowctl (we'll include both in the image)
RUN cargo build -p worker --bin worker --release
RUN cargo build -p postgresflow --bin pgflowctl --release

# ---- runtime ----
FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /app/target/release/worker ./worker
COPY --from=build /app/target/release/pgflowctl ./pgflowctl

EXPOSE 3003

CMD ["./worker"]
