# motion

motion is an open-source engine for making motion graphics and video, designed to be driven by an AI agent.

**Status:** early development. There is nothing to install yet.

## Documentation

- [Documentation index](docs/README.md): start here.
- [Overview](docs/design/overview.md): what motion is and who it is for.
- [Rendering](docs/design/rendering.md): how a scene becomes a video file.
- [Architecture](ARCHITECTURE.md): the crates and how they fit together.
- [Contributing](CONTRIBUTING.md): how to send a change.

## Building from source

You need the Rust toolchain; `rust-toolchain.toml` pins the version and `rustup` installs it for you.

```
cargo test --workspace
```

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT licence](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution you submit for inclusion in this project is dual licensed as above, with no additional terms or conditions.
