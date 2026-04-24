# Contributing to Penumbra

This document outlines the guidelines and best practices for contributing to Penumbra (core library), Antumbra (TUI), and `penumbra-gui` (desktop GUI).
This document assumes familiarity with Git, GitHub, and Rust.

First off, thank you for considering contributing to Penumbra, whether it's through code, documentation, or reporting issues.

---

## Table of Contents

- Prologue:
  - [Code of Conduct](#code-of-conduct)
  - [What to expect](#what-to-expect)
- [Getting Started](#getting-started)
  - [Setting up your development environment](#setting-up-your-development-environment)
  - [Building Penumbra and Antumbra](#building-penumbra-and-antumbra)
- [Issues](#issues)
- [Pull Requests](#pull-requests)
- [Documentation](#documentation)
- [Style Guide](#style-guide)

---

## Code of Conduct

Penumbra exists both as a project and as a learning resource.
As such, please be kind, patient, and respectful to others.

Everyone is here to learn, collaborate, and improve.
Harassment, discrimination, or any form of disrespectful behavior will not be tolerated, just be a decent human being.

## What to expect

All contributions are welcome, but make sure to follow the guidelines below:
- PRs should be focused and address a single issue or feature, and should not go outside its scope (core, tui, gui, docs)
- All contributions will be reviewed, and feedback may be provided.
- Requests for changes may be made before merging, especially if the contribution doesn't follow the [style guide] or has other issues.
- Documentation changes are encouraged, especially if they improve clarity or add missing information, or fix typos.

## Getting Started

To contribute to Penumbra, you'll need to set up your development environment.

### Setting up your development environment

1. Fork Penumbra
2. Clone your forked repository:
   ```bash
   git clone https://github.com/<yourusername>/penumbra
   # or using SSH
   git clone git@github.com:<yourusername>/penumbra
   ```
3. Install the toolchain:

Depending on your OS, you can follow these instructions

### NixOS

If you're using NixOS, you can use the provided `flake.nix` to set up your environment:

```bash
# Enter the development environment
nix develop
# Run the project
nix run
```

#### Arch Linux

```bash
(sudo) pacman -S rustup rust-analyzer systemd glib pkgconf libusb
rustup install nightly
rustup component add --toolchain nightly clippy rustfmt
```

#### Other

Follow the instructions on the [Rust website](https://www.rust-lang.org/tools/install) to install Rust and Cargo.
Additionally, install the following dependencies:
* libudev-dev
* Rust analyzer
* glib
* pkg-config
* systemd-dev

### Building Penumbra, Antumbra, and penumbra-gui

To build the whole workspace, navigate to the project directory and run:

```bash
cargo build
```

This will compile the core library, the TUI (`antumbra`), and the GUI (`penumbra-gui`).
To run Antumbra (TUI / CLI), use:

```bash
cargo run -p antumbra
```

To run the GUI, use:

```bash
cargo run -p penumbra-gui
```

## Issues

When reporting issues, please provide as much detail as possible.
Use the provided issue templates for bug reports and feature requests.

## Pull Requests

When submitting a pull request, please ensure that:
- Your PR is based on the latest `main` branch.
- Your changes are well-documented and tested.
- You have followed the [style guide] for code formatting and conventions.
- You have filled out the PR template completely.
- Reference any related issues in your PR description.

## Documentation

Documentation is the main learning resource for Penumbra.
When contributing to documentation, please ensure that:
- Your changes improve clarity and accuracy.
- You follow the existing style and format.
- You use proper grammar and spelling.

## Style Guide

When contributing code, please adhere to the following style guidelines:
- Follow Rust's standard formatting conventions. Use `rustfmt` to format your code.
- Write clear and concise comments where necessary.
- Use meaningful variable and function names.
- Ensure that your code is modular and reusable where possible.
- Use `clippy` to catch common mistakes and improve code quality.

Commits should follow this conventional commit style:
```
<domain>: <short description>
[optional body]
```

Where `<domain>` is one of:
- `tui`: Changes related to the Antumbra TUI
- `gui`: Changes related to the `penumbra-gui` desktop GUI
- `core`: Changes related to the Penumbra core library
- `docs`: Changes related to documentation
- `scripts`: Changes related to scripts used for development or testing
- `ci`: Changes related to workflows
- `nix`: Changes related to Nix flake or development environment
- `meta`: Changes related to project metadata (e.g., README, LICENSE, CONTRIBUTING)
- `build`: Changes related to build scripts or configuration
- `test`: Changes related to tests or testing infrastructure
- `style`: Changes related to code style or formatting, or just committing cleaned up code
- `deps`: Changes related to dependency updates
- `chore`: Miscellaneous changes that don't fit into other categories

Examples:
```
tui: Add file explorer component
core: Fix handshake on MT6768 devices
docs: Add information about Download Agent
deps: Update tokio to ...
```

When needed to specify further, you can use a scope in parentheses after the domain, e.g., `deps(core): Update thiserror to ...`
Generally, avoid subdomains unless necessary.

```
<domain>(subdomain): <short description>
[optional body]
```

For more complex commits, it is suggested to add a body to the commit message explaining in detail what and why changes were made.
