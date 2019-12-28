FROM clux/muslrust:latest
ADD Cargo.* ./
RUN mkdir src && \
    echo 'fn main(){}' > src/main.rs && \
    cargo fetch
RUN cargo build --release --bin swisher --features=hyper,path-tree,tokio,pretty_env_logger
ADD . .
RUN cargo build --release --bin swisher --features=hyper,path-tree,tokio,pretty_env_logger && \
    mv target/*-musl/release/swisher /swisher

FROM busybox:1-musl
COPY --from=0 /swisher .
CMD ["./swisher"]
