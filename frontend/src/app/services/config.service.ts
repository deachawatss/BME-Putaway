import { Injectable } from '@angular/core';
import { environment } from '../../environments/environment';

export interface AppConfig {
  apiUrl: string;
  production: boolean;
  frontendHost: string;
  frontendPort: number;
  enableDebug: boolean;
  enableInventoryAlerts: boolean;
}

@Injectable({
  providedIn: 'root'
})
export class ConfigService {
  private config: AppConfig;

  constructor() {
    // Load configuration from environment (which gets values from .env)
    this.config = {
      apiUrl: environment.apiUrl,
      production: environment.production,
      frontendHost: environment.frontendHost,
      frontendPort: environment.frontendPort,
      enableDebug: environment.enableDebug,
      enableInventoryAlerts: environment.enableInventoryAlerts
    };

    if (this.config.enableDebug && !this.config.production) {
      console.log('[ConfigService] ℹ️ App Configuration:', this.config);
    }
  }

  getConfig(): AppConfig {
    return this.config;
  }

  getApiUrl(): string {
    return this.config.apiUrl;
  }

  isProduction(): boolean {
    return this.config.production;
  }

  isInventoryAlertsEnabled(): boolean {
    return this.config.enableInventoryAlerts;
  }
}