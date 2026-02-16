# ============================================================
# Putaway Frontend - Angular Dockerfile
# Simple Node.js serve for internal apps (no nginx)
# Uses build args from root .env via docker-compose
# ============================================================

FROM node:20-slim

WORKDIR /app

# Accept build argument for API URL
ARG API_BASE_URL=http://localhost:4400
ARG FRONTEND_PORT=4200

# Persist FRONTEND_PORT to runtime
ENV FRONTEND_PORT=${FRONTEND_PORT}

# Copy package files
COPY package*.json ./

# Install dependencies
RUN npm ci --include=dev

# Install a simple static server
RUN npm install -g serve

# Copy source code
COPY . .

# Extract host from API_BASE_URL and generate configs with proper CSP
RUN API_HOST=$(echo "${API_BASE_URL}" | sed -n 's|http://\([^:]*\):.*|\1|p') && \
    echo "export const environment = {\
  production: true,\
  apiUrl: '${API_BASE_URL}/api',\
  frontendHost: '0.0.0.0',\
  frontendPort: ${FRONTEND_PORT},\
  enableDebug: false,\
  enableInventoryAlerts: false\
};" > src/environments/environment.ts && \
    echo "export const environment = {\
  production: true,\
  apiUrl: '${API_BASE_URL}/api',\
  frontendHost: '0.0.0.0',\
  frontendPort: ${FRONTEND_PORT},\
  enableDebug: false,\
  enableInventoryAlerts: false\
};" > src/environments/environment.prod.ts && \
    echo "export const cspConfig = {\
  connectSrc: 'self http://${API_HOST}:4400 http://${API_HOST}:4400 ws://localhost:${FRONTEND_PORT}',\
  fullCSP: \"default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval' blob:; worker-src 'self' blob:; style-src 'self' 'unsafe-inline' fonts.googleapis.com; font-src 'self' fonts.gstatic.com; img-src 'self' data: blob:; connect-src 'self' http://${API_HOST}:4400 http://${API_HOST}:4400 ws://localhost:${FRONTEND_PORT}\"\
};" > src/environments/csp-config.ts

# Update index.html CSP meta tag - replace entire content attribute
RUN API_HOST=$(echo "${API_BASE_URL}" | sed -n 's|http://\([^:]*\):.*|\1|p') && \
    CSP_CONTENT="default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval' blob:; worker-src 'self' blob:; style-src 'self' 'unsafe-inline' fonts.googleapis.com; font-src 'self' fonts.gstatic.com; img-src 'self' data: blob:; connect-src 'self' http://${API_HOST}:4400 http://${API_HOST}:4400 ws://localhost:${FRONTEND_PORT}" && \
    sed -i 's|<meta http-equiv="Content-Security-Policy" content="[^"]*">|<meta http-equiv="Content-Security-Policy" content="'"$CSP_CONTENT"'">|' src/index.html

# Build for production
RUN npm run build:prod

# Expose port
EXPOSE ${FRONTEND_PORT}

# Serve the built app
CMD serve -s dist/frontend/browser -l ${FRONTEND_PORT}
