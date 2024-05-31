########################################################################

[private]
default:
    @just --list

########################################################################

FORMAT_JUSTFILE:
    just \
        --fmt \
        --unstable

########################################################################

@PUBLISH:
    cargo publish \
        --registry "crates-io"

########################################################################

# https://github.com/rust-lang/miri
# https://rust-lang.github.io/rustup-components-history/x86_64-apple-darwin.html
MIRI_INSTALL:
    # cargo update
    rustup default nightly
    rustup +nightly component add miri

# Для отключения изоляции в miri выставляем переменную окружения
# $MIRIFLAGS="-Zmiri-disable-isolation"
MIRI_TEST:
    MIRIFLAGS="-Zmiri-disable-isolation" \
    cargo miri test