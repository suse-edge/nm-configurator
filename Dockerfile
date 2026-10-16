FROM registry.suse.com/bci/rust:1.98 AS build

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked

FROM scratch
COPY --from=build /src/target/release/nmc /nmc
