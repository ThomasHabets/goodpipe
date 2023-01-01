## Build static

```
rustup target add x86_64-unknown-linux-musl
cargo build --release --target=x86_64-unknown-linux-musl
```

Build output will now be
`target/x86_64-unknown-linux-musl/release/wp`.
