# The site of Facile (site/), served by Facile itself.
#   docker build -t facile-site . && docker run -p 3000:3000 facile-site
FROM rust:1-bookworm AS build
RUN rustup target add wasm32-unknown-unknown
WORKDIR /src
COPY . .
# The browser's WebAssembly, then the server that embeds it.
RUN cargo build -p facile-client --target wasm32-unknown-unknown --profile wasm \
    && cp target/wasm32-unknown-unknown/wasm/facile_client.wasm crates/facile/assets/facile.wasm \
    && cargo build --release -p facile --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/facile /usr/local/bin/facile
COPY site /site
ENV PORT=3000
EXPOSE 3000
CMD ["facile", "serve", "/site"]
