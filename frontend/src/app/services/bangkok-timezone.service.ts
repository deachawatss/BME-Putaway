import { Injectable } from '@angular/core';

/**
 * Service for handling Bangkok timezone operations
 * Provides consistent timezone conversion and formatting across the application
 */
@Injectable({
  providedIn: 'root'
})
export class BangkokTimezoneService {
  private readonly BANGKOK_TIMEZONE = 'Asia/Bangkok';
  private readonly UTC_OFFSET_HOURS = 7;

  /**
   * Convert UTC date to Bangkok timezone
   */
  utcToBangkok(utcDate: Date): Date {
    return new Date(utcDate.getTime() + (this.UTC_OFFSET_HOURS * 60 * 60 * 1000));
  }

  /**
   * Convert Bangkok date to UTC
   */
  bangkokToUtc(bangkokDate: Date): Date {
    return new Date(bangkokDate.getTime() - (this.UTC_OFFSET_HOURS * 60 * 60 * 1000));
  }

  /**
   * Get current Bangkok time
   */
  nowInBangkok(): Date {
    return this.utcToBangkok(new Date());
  }

  /**
   * Format date for display with Bangkok timezone indicator
   */
  formatForDisplay(date: Date | string, includeTime: boolean = true): string {
    let targetDate: Date;
    
    if (typeof date === 'string') {
      targetDate = new Date(date);
    } else {
      targetDate = date;
    }

    // Convert to Bangkok time if it's UTC
    const bangkokDate = this.utcToBangkok(targetDate);
    
    const options: Intl.DateTimeFormatOptions = {
      timeZone: this.BANGKOK_TIMEZONE,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      ...(includeTime && {
        hour: '2-digit',
        minute: '2-digit',
        hour12: false
      })
    };

    const formatted = bangkokDate.toLocaleDateString('en-CA', options);
    return includeTime ? `${formatted} +07` : `${formatted} (Bangkok Time)`;
  }

  /**
   * Format date for form inputs (YYYY-MM-DD)
   */
  formatForInput(date: Date | string): string {
    let targetDate: Date;
    
    if (typeof date === 'string') {
      targetDate = new Date(date);
    } else {
      targetDate = date;
    }

    const bangkokDate = this.utcToBangkok(targetDate);
    return bangkokDate.toISOString().split('T')[0];
  }

  /**
   * Format date for API calls (RFC3339 format with Bangkok timezone)
   */
  formatForApi(date: Date): string {
    const bangkokDate = this.utcToBangkok(date);
    const year = bangkokDate.getFullYear();
    const month = String(bangkokDate.getMonth() + 1).padStart(2, '0');
    const day = String(bangkokDate.getDate()).padStart(2, '0');
    const hours = String(bangkokDate.getHours()).padStart(2, '0');
    const minutes = String(bangkokDate.getMinutes()).padStart(2, '0');
    const seconds = String(bangkokDate.getSeconds()).padStart(2, '0');
    
    return `${year}-${month}-${day}T${hours}:${minutes}:${seconds}+07:00`;
  }

  /**
   * Parse date string assuming it's in Bangkok timezone
   */
  parseFromBangkok(dateString: string): Date {
    const date = new Date(dateString);
    return this.bangkokToUtc(date);
  }

  /**
   * Get current date string in Bangkok timezone for form defaults
   */
  getCurrentDateString(): string {
    return this.formatForInput(this.nowInBangkok());
  }

  /**
   * Validate if a date is in valid Bangkok timezone format
   */
  isValidBangkokDate(dateString: string): boolean {
    try {
      const date = new Date(dateString);
      return !isNaN(date.getTime());
    } catch {
      return false;
    }
  }

  /**
   * Get timezone display string
   */
  getTimezoneDisplay(): string {
    return 'Bangkok Time (UTC+7)';
  }

  /**
   * Get short timezone indicator
   */
  getTimezoneIndicator(): string {
    return '+07';
  }
}