FROM rust:1.98-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim

COPY --from=builder /app/target/release/refinery /usr/local/bin/refinery
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/refinery"]
