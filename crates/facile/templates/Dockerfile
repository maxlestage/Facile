# Builds Facile, then a small image with the binary and the application.
FROM rust:1-bookworm AS build
RUN rustup target add wasm32-unknown-unknown
RUN cargo install --git https://github.com/maxlestage/Facile facile --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/local/cargo/bin/facile /usr/local/bin/facile
WORKDIR /app
COPY . .
ENV PORT=3000
EXPOSE 3000
CMD ["facile", "serve", "."]
