# ── Stage 1: Builder ─────────────────────────────────────────────────────────
FROM rust:1-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends \
    pkg-config \
    curl \
 && rm -rf /var/lib/apt/lists/*

# Cache dependencies separately from source
COPY Cargo.toml Cargo.lock build.rs ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN cargo build --release --locked
RUN rm -f target/release/kvcdr-carb-calculator* target/release/deps/kvcdr_carb_calculator*

# .version is written by CI on push to main; build.rs reads it to embed the SHA
COPY .version* ./
COPY src ./src
RUN cargo build --release --locked

# ── Stage 2: Runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# ca-certificates required by reqwest/rustls to verify Anthropic API TLS
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends \
    ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/kvcdr-carb-calculator ./kvcdr-carb-calculator

RUN useradd --no-create-home --shell /bin/false appuser
USER appuser

EXPOSE 3000
ENTRYPOINT ["./kvcdr-carb-calculator"]
