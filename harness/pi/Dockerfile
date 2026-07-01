FROM node:24-bookworm-slim

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

RUN npm install -g --ignore-scripts @earendil-works/pi-coding-agent

RUN mkdir -p \
      /workspace \
      /home/agent/.pi/agent \
      /home/agent/.pi/extensions \
      /home/agent/.pi/skills \
      /home/agent/.pi/sessions \
      /home/agent/.ssh \
      /home/agent/.cache \
      /tmp/vegasroom

WORKDIR /workspace

CMD ["pi"]
