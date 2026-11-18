# cvmpay dashboard: static build served by unprivileged nginx on port 8080.
# Build context: dashboard/.
FROM node:26-trixie-slim AS build
WORKDIR /src
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY . .
RUN npm run build && rm -f dist/config.json

FROM nginxinc/nginx-unprivileged:1.29-alpine
COPY --from=build /src/dist /usr/share/nginx/html
COPY deploy/nginx.conf /etc/nginx/conf.d/default.conf
COPY --chmod=0755 deploy/config.sh /docker-entrypoint.d/40-cvmpay-config.sh
EXPOSE 8080
