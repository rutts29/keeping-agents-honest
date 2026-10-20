ARG BASE_IMAGE=ubuntu:26.04
FROM ${BASE_IMAGE}

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        bash \
        build-essential \
        ca-certificates \
        curl \
        dnsutils \
        file \
        fd-find \
        git \
        iproute2 \
        jq \
        less \
        netcat-openbsd \
        nodejs \
        npm \
        procps \
        python3 \
        python3-dev \
        python3-pip \
        python3-venv \
        ripgrep \
        rsync \
        shellcheck \
        unzip \
        wget \
        zip \
    && ln -s /usr/bin/fdfind /usr/local/bin/fd \
    && printf '%s\n' 'ignore-scripts=true' 'minimum-release-age=10080' > /etc/npmrc \
    && rm -rf /var/lib/apt/lists/*

# Pinned Rust toolchain. The root is read-only at run time, so CARGO_HOME
# stays unset there and Cargo keeps its registry in $HOME/.cargo (tmpfs).
# Login shells reset PATH, so the proxies are linked into /usr/local/bin.
ARG RUSTUP_VERSION=1.29.1
ARG RUSTUP_SHA256=dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71
ARG RUST_VERSION=1.97.1
ENV RUSTUP_HOME=/usr/local/rustup
RUN curl --proto '=https' --tlsv1.2 -fsSo /tmp/rustup-init \
        "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/x86_64-unknown-linux-gnu/rustup-init" \
    && echo "${RUSTUP_SHA256}  /tmp/rustup-init" | sha256sum -c - \
    && chmod 755 /tmp/rustup-init \
    && CARGO_HOME=/usr/local/cargo /tmp/rustup-init -y --no-modify-path \
        --profile minimal --default-toolchain "${RUST_VERSION}" \
        --component rustfmt --component clippy \
    && rm /tmp/rustup-init \
    && rm -rf /usr/local/rustup/downloads /usr/local/rustup/tmp \
    && ln -s /usr/local/cargo/bin/* /usr/local/bin/ \
    && chmod -R a+rX /usr/local/rustup /usr/local/cargo

ENV HOME=/home/sandbox \
    USER=sandbox \
    LOGNAME=sandbox \
    npm_config_ignore_scripts=true

WORKDIR /workspace
CMD ["sleep", "infinity"]
