# Meowsh

* A small cat that lives at the bottom of your terminal.
* Works with bash & zsh, in kitty & the VS Code terminal.
* Early work. The cat sits & blinks for now.

## Install

* Needs Rust 1.89 or newer.

```sh
cargo install --path .
meowsh install
```

* Open a new terminal & the cat shows up.

## Commands

* `meowsh install`: adds the startup line to your shell settings.
* `meowsh uninstall`: removes it.
* `meowsh preview <name>`: plays one animation, such as `sit`.

## Build & test

```sh
cargo build
cargo test
cargo clippy
```

## Docs

* [Design](docs/DESIGN.md)
* [Architecture](docs/ARCHITECTURE.md)
* [Animation](docs/ANIMATION.md)
* [Roadmap](docs/ROADMAP.md)
* [Contributing](CONTRIBUTING.md)
