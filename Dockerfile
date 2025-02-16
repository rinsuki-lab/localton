FROM rust:1.84-bullseye AS build

RUN apt-get update -y && apt-get install -y --no-install-recommends protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.lock Cargo.toml ./
RUN mkdir src && echo "fn main() {println!(\"if you see this, the build broke\")}" > src/main.rs && cargo build --release && rm -rf src

COPY build.rs ./
COPY proto ./proto
COPY src ./src
RUN touch src/main.rs && cargo build --release

FROM debian:bullseye-slim

WORKDIR /app

COPY --from=build /app/target/release/localton /app/localton

ENV DATA_DIR=/data
VOLUME /data
EXPOSE 3000

CMD ["/app/localton"]