# Thin Pi layer on the shared Vegasroom base image (environment.base_image).
# `vr init --build` builds the base first and passes it in as BASE_IMAGE; the
# base provides the OS tools, environment profile, /home/agent and /workspace.
ARG BASE_IMAGE=vegasroom/base:latest
FROM ${BASE_IMAGE}

ARG PI_CODING_AGENT_VERSION=1.6.0

RUN npm install -g --ignore-scripts "@earendil-works/pi-coding-agent@${PI_CODING_AGENT_VERSION}"

# Keep the baked Pi install in /usr/local as a fallback, but direct any
# in-room `npm install/update -g` operations into a bind-mounted prefix.
# The persistent prefix appears first on PATH so user-updated Pi packages
# are detected and used automatically on subsequent ephemeral containers.
# PATH extends the base's PATH so environment-profile toolchains stay visible.
ENV NPM_CONFIG_PREFIX=/home/agent/.npm-global \
    npm_config_prefix=/home/agent/.npm-global \
    PATH=/home/agent/.npm-global/bin:${PATH}

RUN mkdir -p \
      /home/agent/.pi/agent \
      /home/agent/.pi/extensions \
      /home/agent/.pi/skills \
      /home/agent/.pi/sessions \
      /home/agent/.npm-global

CMD ["pi"]
