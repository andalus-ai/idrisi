FROM rust:1-slim-bookworm AS builder
WORKDIR /app

COPY Cargo.toml rust-toolchain.toml ./
COPY crates/idrisi-api/Cargo.toml crates/idrisi-api/Cargo.toml
COPY crates/idrisi-api/src crates/idrisi-api/src
COPY crates/idrisi-api/migrations crates/idrisi-api/migrations
# Cargo resolves every workspace member even when building one selected package.
COPY crates/idrisi-ingestion/Cargo.toml crates/idrisi-ingestion/Cargo.toml
COPY crates/idrisi-ingestion/src crates/idrisi-ingestion/src

RUN cargo build --release -p idrisi-api -p idrisi-ingestion --bins

FROM debian:bookworm-slim AS runtime
RUN useradd --system --uid 10001 --create-home idrisi && mkdir -p /var/lib/idrisi/ingestion && chown -R idrisi:idrisi /var/lib/idrisi
COPY --from=builder /app/target/release/idrisi-api /usr/local/bin/idrisi-api
COPY --from=builder /app/target/release/idrisi-worker /usr/local/bin/idrisi-worker

USER 10001:10001
EXPOSE 8080
ENV IDRISI_API_BIND=0.0.0.0:8080
ENTRYPOINT ["/usr/local/bin/idrisi-api"]
