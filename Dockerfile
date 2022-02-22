FROM rust:1.55 AS builder

RUN mkdir -p /opt/micropub
WORKDIR /opt/micropub
RUN mkdir micropub-rs
RUN mkdir template

COPY . ./micropub-rs/
WORKDIR /opt/micropub/micropub-rs

# XXX This currently is not sufficient to make ImageMagick available at runtime
# for the server. When using images built from this Dockerfile, exif
# stripping won't work as expected!
# ImageMagick is needed at buildtime for rust-bindgen and later at runtime for
# actually using it. Debian stable does not currently have the 7.x series in
# their repos.
WORKDIR /
# deps list from https://github.com/SoftCreatR/imei/blob/db12bf2e6d9b067108b420dd17ca0af185c8f60c/imei.sh#L446
# added libjpeg62-turbo-dev after seeing libjepg 90 was installed on my mac and
# apt suggested it as a replacement for libjpeg9-dev
RUN apt update && apt install -y imagemagick git curl make cmake automake libtool yasm g++ pkg-config perl libde265-dev libx265-dev libltdl-dev libopenjp2-7-dev liblcms2-dev libbrotli-dev libzip-dev libbz2-dev liblqr-1-0-dev libzstd-dev libgif-dev libjpeg62-turbo-dev libjpeg-dev libopenexr-dev libpng-dev libwebp-dev librsvg2-dev libwmf-dev libxml2-dev libtiff-dev libraw-dev ghostscript gsfonts ffmpeg libpango1.0-dev libdjvulibre-dev libfftw3-dev libgs-dev libgraphviz-dev
# Needs srcs list for build-dep...
#RUN apt-get build-dep -qq imagemagick -y # based on 6.x but installs the deps we need for handling image formats, etc
RUN wget https://github.com/ImageMagick/ImageMagick/archive/refs/tags/7.1.0-19.tar.gz -O ImageMagick-7.1.0-19.tar.gz
RUN ls
RUN tar xfz ImageMagick-7.1.0-19.tar.gz
RUN pwd
WORKDIR ./ImageMagick-7.1.0-19
RUN pwd
RUN ./configure --with-jpeg --with-png && make install -j16 && ldconfig /usr/local/lib
WORKDIR /opt/micropub/micropub-rs

# install rust-bindgen deps
RUN apt update && apt install -y libclang-dev

RUN cargo build --release

FROM debian:stable

RUN apt update && apt install -y openssl libsqlite3-dev ca-certificates build-essential wget imagemagick

RUN wget https://github.com/ImageMagick/ImageMagick/archive/refs/tags/7.1.0-19.tar.gz -O ImageMagick-7.1.0-19.tar.gz

RUN tar xfz ImageMagick-7.1.0-19.tar.gz

WORKDIR ./ImageMagick-7.1.0-19

RUN ./configure && make install -j15 && ldconfig /usr/local/lib

RUN apt remove -y build-essential wget
WORKDIR /opt/micropub/micropub-rs

RUN mkdir -p /opt/micropub/bin
COPY --from=builder /opt/micropub/micropub-rs/target/release/server /opt/micropub/bin/server
