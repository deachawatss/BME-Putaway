import { Injectable } from '@angular/core';
import { environment } from '../../environments/environment';

export enum LogLevel {
  DEBUG = 0,
  INFO = 1,
  WARN = 2,
  ERROR = 3,
  OFF = 4
}

export interface LogConfig {
  enabled: boolean;
  level: LogLevel;
  categories: Set<string>;
  production: boolean;
}

@Injectable({
  providedIn: 'root'
})
export class DebugService {
  private config: LogConfig;

  constructor() {
    this.config = {
      enabled: environment.enableDebug && !environment.production,
      level: environment.production ? LogLevel.WARN : LogLevel.DEBUG,
      categories: new Set(['*']),
      production: environment.production
    };
  }

  private shouldLog(level: LogLevel, category?: string): boolean {
    if (!this.config.enabled || level < this.config.level) {
      return false;
    }

    if (category && this.config.categories.size > 0 && !this.config.categories.has('*')) {
      return this.config.categories.has(category);
    }

    return true;
  }

  private formatMessage(component: string, message: string, data?: any): void {
    const timestamp = new Date().toISOString().substr(11, 12);
    const prefix = `[${timestamp}] [${component}]`;

    if (data !== undefined) {
      console.log(`${prefix} ${message}`, data);
    } else {
      console.log(`${prefix} ${message}`);
    }
  }

  debug(component: string, message: string, data?: any): void {
    if (this.shouldLog(LogLevel.DEBUG, component)) {
      this.formatMessage(component, `🔍 ${message}`, data);
    }
  }

  info(component: string, message: string, data?: any): void {
    if (this.shouldLog(LogLevel.INFO, component)) {
      this.formatMessage(component, `ℹ️ ${message}`, data);
    }
  }

  warn(component: string, message: string, data?: any): void {
    if (this.shouldLog(LogLevel.WARN, component)) {
      console.warn(`[${component}] ⚠️ ${message}`, data || '');
    }
  }

  error(component: string, message: string, data?: any): void {
    if (this.shouldLog(LogLevel.ERROR, component)) {
      console.error(`[${component}] ❌ ${message}`, data || '');
    }
  }

  // Specialized logging methods for common patterns
  stateChange(component: string, action: string, before?: any, after?: any): void {
    if (this.shouldLog(LogLevel.DEBUG, component)) {
      this.debug(component, `🔄 STATE: ${action}`, { before, after });
    }
  }

  apiCall(component: string, method: string, url: string, data?: any): void {
    if (this.shouldLog(LogLevel.DEBUG, component)) {
      this.debug(component, `🌐 API ${method}: ${url}`, data);
    }
  }

  performance(component: string, operation: string, duration?: number): void {
    if (this.shouldLog(LogLevel.DEBUG, component)) {
      const msg = duration ? `⚡ PERF: ${operation} took ${duration}ms` : `⚡ PERF: ${operation}`;
      this.debug(component, msg);
    }
  }

  // Configuration methods
  setLevel(level: LogLevel): void {
    this.config.level = level;
  }

  enableCategory(category: string): void {
    this.config.categories.add(category);
  }

  disableCategory(category: string): void {
    this.config.categories.delete(category);
  }

  setEnabled(enabled: boolean): void {
    this.config.enabled = enabled && !this.config.production;
  }
}