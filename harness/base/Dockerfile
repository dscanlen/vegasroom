# Vegasroom base foundation, shared by every harness image.
#
# `vr init --build` writes this foundation followed by the generated
# `environment:` profile sections (apt/python/go/typescript/rust) to
# ~/.vegasroom/runtime/harness/base/Dockerfile and builds it as
# environment.base_image (default vegasroom/base:latest). Harness images
# then build FROM it and add only their agent.
#
# Contract every harness relies on: Debian bookworm + Node 24, HOME and the
# room user's home at /home/agent, /workspace as the working directory, and
# /root/.ssh symlinked to the mounted /home/agent/.ssh.
FROM node:24-bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive \
    HOME=/home/agent \
    npm_config_update_notifier=false

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      build-essential \
      ca-certificates \
      curl \
      fd-find \
      git \
      openssh-client \
      ripgrep \
    && ln -sf /usr/bin/fdfind /usr/local/bin/fd \
    && rm -rf /var/lib/apt/lists/*

ENV NPM_CONFIG_CACHE=/home/agent/.cache/npm \
    npm_config_cache=/home/agent/.cache/npm

RUN mkdir -p \
      /workspace \
      /home/agent/.cargo \
      /home/agent/.ssh \
      /home/agent/.cache \
      /tmp/vegasroom \
    && rm -rf /root/.ssh \
    && ln -s /home/agent/.ssh /root/.ssh

WORKDIR /workspace
