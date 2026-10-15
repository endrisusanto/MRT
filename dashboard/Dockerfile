FROM node:22-alpine AS runner

WORKDIR /app

COPY package.json package-lock.json* ./
RUN npm install --omit=dev

COPY server.js ./
COPY public ./public

ENV PORT=3010
ENV NODE_ENV=production

EXPOSE 3010

USER node

CMD ["node", "server.js"]
