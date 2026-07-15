FROM node:24-bookworm-slim

ARG PI_CODING_AGENT_VERSION=0.80.7

ENV DEBIAN_FRONTEND=noninteractive \
    HOME=/home/agent \
    npm_config_update_notifier=false

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      ca-certificates \
      curl \
      fd-find \
      git \
      openssh-client \
      ripgrep \
    && ln -sf /usr/bin/fdfind /usr/local/bin/fd \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g --ignore-scripts "@earendil-works/pi-coding-agent@${PI_CODING_AGENT_VERSION}"

# Keep the baked Pi install in /usr/local as a fallback, but direct any
# in-room `npm install/update -g` operations into a bind-mounted prefix.
# The persistent prefix appears first on PATH so user-updated Pi packages
# are detected and used automatically on subsequent ephemeral containers.
ENV NPM_CONFIG_PREFIX=/home/agent/.npm-global \
    npm_config_prefix=/home/agent/.npm-global \
    NPM_CONFIG_CACHE=/home/agent/.cache/npm \
    npm_config_cache=/home/agent/.cache/npm \
    PATH=/home/agent/.npm-global/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN mkdir -p \
      /workspace \
      /home/agent/.pi/agent \
      /home/agent/.pi/extensions \
      /home/agent/.pi/skills \
      /home/agent/.pi/sessions \
      /home/agent/.npm-global \
      /home/agent/.cargo \
      /home/agent/.ssh \
      /home/agent/.cache \
      /tmp/vegasroom \
    && rm -rf /root/.ssh \
    && ln -s /home/agent/.ssh /root/.ssh

WORKDIR /workspace

CMD ["pi"]
