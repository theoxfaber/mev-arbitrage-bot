FROM rust:1.80 as builder

WORKDIR /usr/src/app
COPY . .

RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/mev-arbitrage-bot /usr/local/bin/mev-arbitrage-bot

ENTRYPOINT ["mev-arbitrage-bot"]
