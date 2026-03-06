FROM rust:1-alpine AS builder

RUN apk add --no-cache musl-dev

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN cargo build --release

FROM gcr.io/distroless/static-debian12

COPY --from=builder /app/target/release/tuic-relay /tuic-relay

ENV RELAY_BIND=0.0.0.0:8080
ENV RELAY_DB_PATH=/data/relay.db

EXPOSE 8080
VOLUME ["/data"]

ENTRYPOINT ["/tuic-relay"]
