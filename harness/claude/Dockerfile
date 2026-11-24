# Built FROM the shared Vegasroom base image (environment.base_image), which
# `vr init --build` builds first and passes in as BASE_IMAGE. The default keeps
# this Dockerfile buildable on its own.
ARG BASE_IMAGE=node:24-bookworm-slim
FROM ${BASE_IMAGE}

ARG CLAUDE_CODE_VERSION=2.1.325

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

# The package's postinstall links the platform-specific native binary
# (shipped as an optional dependency) into place, so install scripts must run.
RUN npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}" \
    && npm cache clean --force \
    && claude --version

# Claude Code state (settings, /login credentials, sessions, .claude.json)
# lives in a single bind-mounted CLAUDE_CONFIG_DIR. The image version is
# pinned; in-room self-updates are disabled so rebuilds stay the only
# upgrade path.
ENV CLAUDE_CONFIG_DIR=/home/agent/.claude \
    DISABLE_AUTOUPDATER=1 \
    NPM_CONFIG_CACHE=/home/agent/.cache/npm \
    npm_config_cache=/home/agent/.cache/npm

RUN mkdir -p \
      /workspace \
      /home/agent/.claude \
      /home/agent/.cargo \
      /home/agent/.ssh \
      /home/agent/.cache \
      /tmp/vegasroom \
    && rm -rf /root/.ssh \
    && ln -s /home/agent/.ssh /root/.ssh

WORKDIR /workspace

CMD ["claude"]
