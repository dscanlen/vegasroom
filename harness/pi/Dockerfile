FROM node:24-bookworm-slim

ENV HOME=/home/agent \
    npm_config_update_notifier=false

# M1 runtime dependencies.
# Pi looks for fd and ripgrep at startup. Install them in the image so Pi does
# not try to download helper binaries into persisted state on first launch.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      ca-certificates \
      fd-find \
      git \
      openssh-client \
      ripgrep \
    && ln -sf /usr/bin/fdfind /usr/local/bin/fd \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g --ignore-scripts @earendil-works/pi-coding-agent

# Keep HOME stable at /home/agent even though the container runs as root for M1.
# In rootless Docker, container root is mapped through the rootless user
# namespace and is not host root. This avoids UID/GID bind-mount friction while
# preserving only explicitly mounted state.
RUN mkdir -p \
      /workspace \
      /home/agent/.pi/agent \
      /home/agent/.pi/extensions \
      /home/agent/.pi/skills \
      /home/agent/.pi/sessions \
      /home/agent/.ssh \
      /home/agent/.cache \
    && touch /home/agent/.ssh/known_hosts

WORKDIR /workspace

CMD ["pi"]
