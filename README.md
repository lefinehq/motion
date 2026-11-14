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

## Try it

Build the `motion` tool and point it at the example scene:

```
cargo run -q -p motion-cli -- check examples/launch.json
OK 4

cargo run -q -p motion-cli -- at examples/launch.json 1s title
title x=960 y=260 w=580.8 h=115.2

cp examples/launch.json /tmp/launch.json
cargo run -q -p motion-cli -- edit /tmp/launch.json 'card fill=#FF0000'
OK 4

cargo run -q -p motion-cli -- render examples/launch.json --frames 0-29 --out /tmp/frames
30 frames written to /tmp/frames
```

`render` writes `frame-00000.png` and on, one frame per processor thread at a time; `--frame N` draws one, and with neither flag every frame is drawn. `--quality fast` trades float compositing for speed in previews.

`check` exits 1 when it finds an error. `edit` saves a valid scene even when the scene still has check errors, and then exits 1 as well. Every command exits 2 when the file, the time or the edit line is unusable.

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT licence](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution you submit for inclusion in this project is dual licensed as above, with no additional terms or conditions.
