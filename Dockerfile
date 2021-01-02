FROM rust:1.46

ARG tag=master

RUN mkdir /template
WORKDIR /template
RUN git clone https://github.com/davidwilemski/blue-penguin
WORKDIR /template/blue-penguin
RUN git checkout dtw/tera-support

WORKDIR /
RUN git clone https://github.com/davidwilemski/micropub-rs
WORKDIR /micropub-rs
RUN git checkout master && git pull && git checkout $tag

RUN cargo build --release
