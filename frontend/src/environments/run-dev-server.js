#!/usr/bin/env node

const { spawn } = require('child_process');
const path = require('path');
const dotenv = require('dotenv');

// Load .env file from frontend directory
const envPath = path.resolve(__dirname, '../../.env');
const result = dotenv.config({ path: envPath });

if (result.error) {
  console.warn('Warning: .env file not found at', envPath);
}

// Get environment variables with fallbacks
const host = process.env.FRONTEND_HOST || '0.0.0.0';
const port = process.env.FRONTEND_PORT || '4200';

console.log(`Starting Angular dev server on ${host}:${port}`);

// Run ng serve with the environment-based host and port
const ngServe = spawn('ng', ['serve', '--host', host, '--port', port], {
  stdio: 'inherit',
  shell: true
});

ngServe.on('error', (error) => {
  console.error('Error starting ng serve:', error);
  process.exit(1);
});

ngServe.on('close', (code) => {
  console.log(`ng serve exited with code ${code}`);
  process.exit(code);
});