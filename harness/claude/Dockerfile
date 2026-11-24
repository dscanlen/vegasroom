# Thin Claude Code layer on the shared Vegasroom base image (environment.base_image).
# `vr init --build` builds the base first and passes it in as BASE_IMAGE; the
# base provides the OS tools, environment profile, /home/agent and /workspace.
ARG BASE_IMAGE=vegasroom/base:latest
FROM ${BASE_IMAGE}

ARG CLAUDE_CODE_VERSION=2.1.325

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
    DISABLE_AUTOUPDATER=1

RUN mkdir -p /home/agent/.claude

CMD ["claude"]
