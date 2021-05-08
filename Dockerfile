FROM rust:1.49 AS builder

RUN mkdir -p /opt/micropub
WORKDIR /opt/micropub
RUN mkdir micropub-rs
RUN mkdir template

COPY . ./micropub-rs/
WORKDIR /opt/micropub/micropub-rs

RUN cargo build --release

FROM debian:stable

RUN apt update && apt install -y openssl libsqlite3-dev ca-certificates

RUN mkdir -p /opt/micropub/bin
COPY --from=builder /opt/micropub/micropub-rs/target/release/server /opt/micropub/bin/server
