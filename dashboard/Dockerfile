# deadhop dashboard: static build served by unprivileged nginx on port 8080.
# Build context: the repository root (the dashboard uses packages/client).
FROM node:26-trixie-slim AS build
WORKDIR /src
COPY package.json package-lock.json ./
COPY packages/client/package.json packages/client/
# npm ci needs every workspace's package.json, even ones it does not install.
COPY packages/react/package.json packages/react/
COPY packages/solid/package.json packages/solid/
COPY packages/svelte/package.json packages/svelte/
COPY packages/e2e/package.json packages/e2e/
COPY dashboard/package.json dashboard/
RUN npm ci --no-audit --no-fund --workspace dashboard --workspace packages/client
COPY packages/client/ packages/client/
COPY brand/fonts/ brand/fonts/
COPY brand/icons/ brand/icons/
COPY dashboard/ dashboard/
RUN npm run build --workspace dashboard && rm -f dashboard/dist/config.json

FROM nginxinc/nginx-unprivileged:1.29-alpine
COPY --from=build /src/dashboard/dist /usr/share/nginx/html
COPY dashboard/deploy/nginx.conf /etc/nginx/conf.d/default.conf
COPY --chmod=0755 dashboard/deploy/config.sh /docker-entrypoint.d/40-deadhop-config.sh
EXPOSE 8080
