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

ENV HOME=/home/sandbox \
    USER=sandbox \
    LOGNAME=sandbox \
    npm_config_ignore_scripts=true

WORKDIR /workspace
CMD ["sleep", "infinity"]
