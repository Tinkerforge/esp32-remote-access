FROM rust:1-bookworm

RUN apt update && apt install clang -y
RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
WORKDIR /build
CMD wasm-pack build
