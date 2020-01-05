FROM clux/muslrust:latest
ADD Cargo.* ./
RUN mkdir src && \
    echo 'fn main(){}' > src/main.rs && \
    cargo fetch
RUN cargo build --release --bin swisher --features=bin
ADD . .
RUN cargo build --release --bin swisher --features=bin && \
    mv target/*-musl/release/swisher /swisher

FROM busybox:1
COPY --from=0 /swisher .
CMD ["./swisher"]
