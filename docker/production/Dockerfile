# This is a multi-stage Dockerfile.
# Everything that is not copied from the other stages is ommited in the final stage.

# Build stage for the backend binary
FROM rust:1-bookworm AS backend-build

RUN useradd --create-home --shell /sbin/nologin builder
WORKDIR /home/builder

COPY --chown=builder:builder backend ./backend
COPY --chown=builder:builder db_connector ./db_connector

USER builder
WORKDIR /home/builder/backend
RUN cargo build --release

# Build stage for the wg-webclient package
FROM rust:1-bookworm AS wg-webclient-build

RUN apt update && apt install -y clang && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /sbin/nologin builder
WORKDIR /home/builder

USER builder
RUN cargo install wasm-pack

COPY --chown=builder:builder wg-webclient ./wg-webclient
WORKDIR /home/builder/wg-webclient
RUN wasm-pack build

# Frontend build stage
FROM node:22 AS frontend-build

ARG BRAND
ENV BRAND=${BRAND}

RUN useradd --create-home --shell /sbin/nologin builder
WORKDIR /home/builder

COPY --chown=builder:builder frontend ./frontend
COPY --from=wg-webclient-build --chown=builder:builder /home/builder/wg-webclient ./wg-webclient

USER builder
WORKDIR /home/builder/frontend
RUN npm ci && npm run build

# Final stage
FROM debian:13 as runner
RUN apt update && apt install -y libpq-dev libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /sbin/nologin appuser

WORKDIR /home/appuser
COPY --from=backend-build --chown=appuser:appuser /home/builder/backend/target/release/backend .
COPY --from=frontend-build --chown=appuser:appuser /home/builder/frontend/dist /public

RUN mkdir /certs && chown appuser:appuser /certs

USER appuser

ENV DATABASE_URL=${DATABASE_URL}
ENV JWT_SECRET=${JWT_SECRET}
ENV EMAIL_USER=${EMAIL_USER}
ENV EMAIL_PASS=${EMAIL_PASS}
ENV EMAIL_RELAY=${EMAIL_RELAY}
ENV EMAIL_RELAY_PORT=${EMAIL_RELAY_PORT}
ENV FRONTEND_URL=${FRONTEND_URL}
ENV SERVER_NAME=${SERVER_NAME}
ENV MONITORING_EMAIL=${MONITORING_EMAIL}
ENV SENDER_EMAIL=${SENDER_EMAIL}
ENV SENDER_NAME=${SENDER_NAME}
ENV BRAND=${BRAND}
ENV STATIC_FILES_DIR=/public
ENV TLS_CERT_PATH=${TLS_CERT_PATH}
ENV TLS_KEY_PATH=${TLS_KEY_PATH}

CMD ["./backend"]
