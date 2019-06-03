FROM rust:1.35 as build

WORKDIR /opt/testapp

RUN USER=root cargo init

COPY Cargo.toml .
COPY Cargo.lock .

RUN cargo build --release
RUN cargo build 

COPY src/* ./src/
COPY tests/* ./tests/

RUN cargo test --all
RUN cargo build --release

FROM debian:stretch-slim

COPY --from=build /opt/testapp/target/release/trlogic_test /usr/local/bin/

RUN trlogic_test --help

CMD ["trlogic_test", "--host=0.0.0.0", "--port=8000", "--upload=/var/lib/trlogic_test/uploads/"]
