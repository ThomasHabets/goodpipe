# Wrap pipe

This wraps the pipe by embedding metadata in the stdout and stdin streams, so
that the final step in a pipeline can tell the difference between EOF coming
from failure and EOF coming from actual EOF.

## Build static

```
rustup target add x86_64-unknown-linux-musl
cargo build --release --target=x86_64-unknown-linux-musl
```

Build output will now be
`target/x86_64-unknown-linux-musl/release/wp`.
