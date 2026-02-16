import { bootstrapApplication } from '@angular/platform-browser';
import { appConfig } from './app/app.config';
import { App } from './app/app';

// Suppress browser extension errors that don't affect our application
window.addEventListener('error', (event): boolean => {
  if (event.error?.message?.includes('message channel closed before a response was received')) {
    // This is a browser extension error, not our code
    event.preventDefault();
    return false;
  }
  return true;
});

window.addEventListener('unhandledrejection', (event): boolean => {
  if (event.reason?.message?.includes('message channel closed before a response was received')) {
    // This is a browser extension error, not our code
    event.preventDefault();
    return false;
  }
  return true;
});

bootstrapApplication(App, appConfig)
  .catch((err) => console.error(err));
