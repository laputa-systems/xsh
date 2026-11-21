FROM node@sha256:6bd9ab43c0ec0190ca7b2d193906cb8b7edf6e7f4afa43ddeccd6452710bffb5
WORKDIR /opt/ts
COPY package.json package-lock.json ./
RUN npm ci --ignore-scripts --no-audit --no-fund && node_modules/.bin/tsc --version
WORKDIR /
