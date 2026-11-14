FROM rust:1-slim-bookworm AS builder
WORKDIR /app

COPY Cargo.toml rust-toolchain.toml ./
COPY crates/idrisi-api/Cargo.toml crates/idrisi-api/Cargo.toml
COPY crates/idrisi-api/src crates/idrisi-api/src
COPY crates/idrisi-api/migrations crates/idrisi-api/migrations

RUN cargo build --release -p idrisi-api

FROM debian:bookworm-slim AS runtime
RUN useradd --system --uid 10001 --create-home idrisi
COPY --from=builder /app/target/release/idrisi-api /usr/local/bin/idrisi-api

USER 10001:10001
EXPOSE 8080
ENV IDRISI_API_BIND=0.0.0.0:8080
ENTRYPOINT ["/usr/local/bin/idrisi-api"]
