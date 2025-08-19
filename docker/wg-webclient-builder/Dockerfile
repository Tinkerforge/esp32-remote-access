FROM rust:1-bookworm

RUN apt update && apt install -y clang && rm -rf /var/lib/apt/lists/*

ARG USER_ID=1000
ARG GROUP_ID=1000

RUN addgroup --gid ${GROUP_ID} builder && \
	adduser --disabled-password --gecos "" \
			--uid ${USER_ID} --ingroup builder \
			--shell /bin/sh --home /home/builder --quiet builder

USER builder

RUN cargo install wasm-pack

WORKDIR /build
CMD ["wasm-pack", "build"]
