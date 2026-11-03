# Contributing

* Short rules for changing Meowsh's code.
* Run the checks at the bottom before every commit.

## Contents

* [Agents](#agents)
* [Where Code Goes](#where-code-goes)
* [Errors](#errors)
* [Clones](#clones)
* [Names](#names)
* [Comments](#comments)
* [Tests](#tests)
* [Art](#art)
* [Before You Commit](#before-you-commit)
* [Pull Requests](#pull-requests)

## Agents

* AI help is welcome. Treat what it gives you as a first draft from someone who does not know this project.
* You are the author. You answer for every line, whoever or whatever wrote it.
* Pull requests that break these rules will be closed without review.

**Check the output**

* Read every line before you commit it. If you cannot explain why a line is there, take it out.
* Check it against the rules in this file. Agents often use `unwrap`, add clones, mix the three parts, or invent short names.
* Check that every crate, function & setting it uses really exists. Agents make them up.
* Before an agent writes something from scratch, have it check for a crate that already does the job, such as on crates.io or in the libraries listed in [ARCHITECTURE.md](docs/ARCHITECTURE.md#libraries). Use a well kept crate instead of rewriting it.
* Do not trust an agent that says the tests pass. Run the checks below yourself.
* Watch for changes you did not ask for, such as edits to other files, new crates, or removed tests.
* Keep agent changes small, so you can really review them. Split a big one into several commits.

**Commits & pull requests**

* Make every commit yourself, from your own git account, with your own name & email.
* Agents must not commit, push, or open pull requests for you.
* No agent may appear as an author, committer, or `Co-authored-by` line.
* If an agent wrote a large part of a change, say so in the pull request.

**Art**

* Do not commit AI made art. Draw the cat & its animations yourself in Blender.

## Where Code Goes

* Meowsh has three parts that must stay apart. See [ARCHITECTURE.md](docs/ARCHITECTURE.md#the-three-parts).

| Part | Folder | Rule |
| --- | --- | --- |
| **Terminal** | `src/term/` | Knows terminals. Never uses anything from `cat/` or `art/`. |
| **Cat** | `src/cat/` | Knows the cat. Never uses anything from `term/`. Never reads the clock or the screen itself. |
| **Art** | `src/art/` | Loads & prepares frames. Never uses `term/` or `cat/`. |

* `anim.rs`, `scene.rs` & `env.rs` hold plain shared data. Every part may use them.
* Only `src/app.rs` & `src/app/` join the parts together.
* If a change needs to break one of these rules, open an issue first.
* Small, focused files. Use `foo.rs` with a `foo/` folder next to it, not `mod.rs`.

## Errors

* Do not use `unwrap` or `expect`, in tests too. Clippy blocks them.
* Use `?` & add context, such as `.context("could not open PTY")`.
* When something being missing is normal, use `match` or `if let` on the `Option`.
* Do not invent a default value just to get past an error.

## Clones

* Borrow with `&` when the caller still needs the value.
* Move the value when the caller is done with it.
* Clone only when two owners really need it & say why in a comment.
* Do not hide a copy with `.to_owned()` or `.to_vec()`.

## Names

* Clear, full names, such as `shell_path` or `early_input`. No short or clever names.
* Put repeated or unclear values in a named `const` with a short `///` comment, not inline.
* Terminal codes go in `src/term/escape.rs`. Environment variable names go in `src/env.rs`.
* Tests use the same constants. They do not copy raw bytes.

## Comments

* Every file starts with a `//!` line that says what it is for.
* Every public item has a short `///` comment.
* Comments explain why, in plain words & only where the code is not obvious.
* Every `unsafe` block has a `// SAFETY:` comment.
* No em dashes, in code, comments, docs, or commit messages.

## Tests

* Tests return `Result` & use `?` instead of `unwrap`.
* Keep unit tests at the bottom of the file they check. Tests that run the whole program go in `tests/`.
* Everything in `cat/` must be testable with no terminal & no real clock.

## Art

* The source file for every animation is committed next to its frames. See [ANIMATION.md](docs/ANIMATION.md).
* Exported frames must follow the frame rules in that doc. A test checks them.
* Only commit art you made yourself, or art you have the right to share.

## Before You Commit

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
```

* Keep commits small: one change per commit.
* Start the message with `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `art:`, or `chore:`.
* Keep the message short & plain.

## Pull Requests

* One topic per pull request.
* Say what changed & how you checked it. If you ran Meowsh in a terminal, say which one.
* Link the issue it fixes, such as `Fixes #12`.
