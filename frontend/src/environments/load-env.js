#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

// Load dotenv
const dotenv = require('dotenv');

// Load .env file from frontend directory
const envPath = path.resolve(__dirname, '../../.env');
const result = dotenv.config({ path: envPath });

if (result.error) {
  console.warn('Warning: .env file not found at', envPath);
}

// Environment variables with fallbacks
const envVars = {
  API_URL: process.env.API_URL || 'http://localhost:4455/api',
  FRONTEND_HOST: process.env.FRONTEND_HOST || '0.0.0.0',
  FRONTEND_PORT: process.env.FRONTEND_PORT || '4200',
  PRODUCTION: process.env.PRODUCTION || 'false',
  DEBUG: process.env.DEBUG || 'false',
  ENABLE_MOCK_DATA: process.env.ENABLE_MOCK_DATA || 'false',
  ENABLE_INVENTORY_ALERTS: process.env.ENABLE_INVENTORY_ALERTS || 'false',
  // CSP Configuration
  CSP_API_HOST: process.env.CSP_API_HOST || 'localhost',
  CSP_API_PORT: process.env.CSP_API_PORT || '4400',
  CSP_NETWORK_HOST: process.env.CSP_NETWORK_HOST || 'localhost',
  CSP_NETWORK_PORT: process.env.CSP_NETWORK_PORT || '4400',
  CSP_WS_PORT: process.env.CSP_WS_PORT || '4200'
};

// Generate environment.ts content
const envContent = `// Auto-generated environment file from .env
// DO NOT EDIT - Edit frontend/.env instead

export const environment = {
  production: ${envVars.PRODUCTION},
  apiUrl: '${envVars.API_URL}',
  frontendHost: '${envVars.FRONTEND_HOST}',
  frontendPort: ${envVars.FRONTEND_PORT},
  enableDebug: ${envVars.DEBUG},
  enableMockData: ${envVars.ENABLE_MOCK_DATA},
  enableInventoryAlerts: ${envVars.ENABLE_INVENTORY_ALERTS}
};
`;

// Write to environment.ts
const environmentPath = path.resolve(__dirname, 'environment.ts');
fs.writeFileSync(environmentPath, envContent, 'utf8');

// Generate environment.prod.ts with same content but production-specific settings
const envProdContent = `// Auto-generated production environment file from .env
// DO NOT EDIT - Edit frontend/.env instead

export const environment = {
  production: true,
  apiUrl: '${envVars.API_URL}',
  frontendHost: '${envVars.FRONTEND_HOST}',
  frontendPort: ${envVars.FRONTEND_PORT},
  enableDebug: false, // Always false in production
  enableMockData: false, // Always false in production
  enableInventoryAlerts: ${envVars.ENABLE_INVENTORY_ALERTS}
};
`;

// Write to environment.prod.ts
const environmentProdPath = path.resolve(__dirname, 'environment.prod.ts');
fs.writeFileSync(environmentProdPath, envProdContent, 'utf8');

// Generate CSP configuration
const cspConfig = `// Auto-generated CSP configuration from .env
// DO NOT EDIT - Edit frontend/.env instead

export const cspConfig = {
  connectSrc: 'self http://${envVars.CSP_API_HOST}:${envVars.CSP_API_PORT} http://${envVars.CSP_NETWORK_HOST}:${envVars.CSP_NETWORK_PORT} ws://localhost:${envVars.CSP_WS_PORT}',
  fullCSP: "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval' blob:; worker-src 'self' blob:; style-src 'self' 'unsafe-inline' fonts.googleapis.com; font-src 'self' fonts.gstatic.com; img-src 'self' data: blob:; connect-src 'self' http://${envVars.CSP_API_HOST}:${envVars.CSP_API_PORT} http://${envVars.CSP_NETWORK_HOST}:${envVars.CSP_NETWORK_PORT} ws://localhost:${envVars.CSP_WS_PORT}"
};
`;

// Write CSP configuration
const cspConfigPath = path.resolve(__dirname, 'csp-config.ts');
fs.writeFileSync(cspConfigPath, cspConfig, 'utf8');

// Update index.html with dynamic CSP
const indexHtmlPath = path.resolve(__dirname, '../../src/index.html');
if (fs.existsSync(indexHtmlPath)) {
  let indexContent = fs.readFileSync(indexHtmlPath, 'utf8');
  
  // Replace the CSP meta tag with dynamic values
  const newCSP = `"default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval' blob:; worker-src 'self' blob:; style-src 'self' 'unsafe-inline' fonts.googleapis.com; font-src 'self' fonts.gstatic.com; img-src 'self' data: blob:; connect-src 'self' http://${envVars.CSP_API_HOST}:${envVars.CSP_API_PORT} http://${envVars.CSP_NETWORK_HOST}:${envVars.CSP_NETWORK_PORT} ws://localhost:${envVars.CSP_WS_PORT}"`;
  
  // Update CSP meta tag
  indexContent = indexContent.replace(
    /<meta http-equiv="Content-Security-Policy" content="[^"]*">/,
    `<meta http-equiv="Content-Security-Policy" content=${newCSP}>`
  );
  
  fs.writeFileSync(indexHtmlPath, indexContent, 'utf8');
  console.log('Updated index.html with dynamic CSP configuration');
}

console.log('Environment variables loaded and written to environment.ts and environment.prod.ts');
console.log('CSP configuration generated');
console.log('Configuration:', envVars);
