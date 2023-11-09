FROM registry.suse.com/bci/rust:1.73

COPY . /
WORKDIR /

RUN cargo build --release --config net.git-fetch-with-cli=true
