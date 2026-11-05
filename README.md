# Meowsh

* A small cat that lives in your terminal.
* Meowsh wraps your shell & keeps the bottom rows of the window for a cat. It sleeps, wanders, plays & eats there & reacts to what you do.
* Works with bash & zsh, in kitty & the VS Code terminal.

## Status

* Early work. Meowsh runs your shell inside its PTY & keeps the bottom rows as a bar, but the cat there is only a still white oval for now.
* Done so far: setup, the shell wrapper with `meowsh install` & `meowsh uninstall`, & the bar.
* Next: animations.
* See the [roadmap](docs/ROADMAP.md) for the order of work.

## Docs

* [Design](docs/DESIGN.md): what Meowsh is & how the cat behaves.
* [Architecture](docs/ARCHITECTURE.md): how it is built.
* [Animation](docs/ANIMATION.md): how the cat is drawn & how its frames get into the program.
* [Animations](docs/ANIMATIONS.md): every animation planned for the cat & what each one is for.
* [Roadmap](docs/ROADMAP.md): what comes first & what comes next.
* [Contributing](CONTRIBUTING.md): rules for code, commits & agents.

## History

* Meowsh is a redo of an earlier prototype of the same idea.
* Most of the terminal code came over from that prototype & was cleaned up: the shell wrapper, the PTY, raw mode, the startup scripts & install.
* The cat, its brain & all of its art start fresh.

## Names

* **Meowsh** is the project. A cat (meow) that wraps your shell (sh).
* `meowsh` is both the command you type & the package on crates.io.
