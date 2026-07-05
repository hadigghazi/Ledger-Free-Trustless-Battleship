# Build the relay only. Build from the repo root so Cargo can read the
# workspace manifest:
#   docker build -f relay/Dockerfile -t zk-battleship-relay .
#
# The relay has no dependency on the game logic or RISC Zero, so this image
# never needs the zkVM toolchain and stays tiny.
FROM rust:1-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p relay

FROM debian:stable-slim
RUN useradd -m relay
USER relay
COPY --from=build /src/target/release/zk-battleship-relay /usr/local/bin/zk-battleship-relay
ENV PORT=9000
EXPOSE 9000
CMD ["zk-battleship-relay"]
