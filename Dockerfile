FROM rust:1.49

ARG tag=master

RUN mkdir -p /opt/micropub
WORKDIR /opt/micropub
RUN mkdir template

RUN git clone https://github.com/davidwilemski/micropub-rs
WORKDIR /opt/micropub/micropub-rs
RUN git checkout master && git pull && git checkout $tag

RUN cargo build --release
