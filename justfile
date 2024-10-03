default:
    @just --list

check:
    cargo lclippy

remove-links:
    rm ${PRJ_ROOT}/target/debug/task || true
    rm ${PRJ_ROOT}/target/debug/tw || true

build: remove-links
    cargo lbuild
    ln -s ${PRJ_ROOT}/target/debug/taskhelper ${PRJ_ROOT}/target/debug/task
    ln -s ${PRJ_ROOT}/target/debug/taskhelper ${PRJ_ROOT}/target/debug/tw

run: build
    RUST_LOG=debug cargo lrun

doctest:
    # cargo-nextest doesn't yet support doctests
    # https://github.com/nextest-rs/nextest/issues/16
    cargo ltest --doc

test:
    cargo lbuild --tests
    cargo nextest run --all-targets

fmt:
    treefmt

docs:
    oranda build
    oranda serve

cov:
    nix build .#packages.x86_64-linux.llm-coverage
