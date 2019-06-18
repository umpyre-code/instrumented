FROM rust

WORKDIR /work

ENV SCCACHE_VER=0.2.8

RUN wget -q https://github.com/mozilla/sccache/releases/download/${SCCACHE_VER}/sccache-${SCCACHE_VER}-x86_64-unknown-linux-musl.tar.gz -O sccache-${SCCACHE_VER}-x86_64-unknown-linux-musl.tar.gz \
  && tar xf sccache-${SCCACHE_VER}-x86_64-unknown-linux-musl.tar.gz \
  && cp sccache-${SCCACHE_VER}-x86_64-unknown-linux-musl/sccache /usr/bin \
  && apt-get update \
  && apt-get install -y cmake curl \
  && rm -rf /var/lib/apt/lists/*

RUN rustup component add clippy \
  && rustup component add rustfmt \
  && rustup target add x86_64-unknown-linux-gnu \
  && RUSTFLAGS="--cfg procmacro2_semver_exempt" cargo install cargo-tarpaulin \
  && rm -rf $CARGO_HOME/registry \
  && rm -rf $CARGO_HOME/git

ENV RUSTC_WRAPPER=sccache
