import { Injectable } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { BehaviorSubject, Observable, of, throwError } from 'rxjs';
import { catchError, map } from 'rxjs/operators';
import { ConfigService } from './config.service';
import { DebugService } from './debug.service';

export interface User {
  username: string;
  display_name: string;
  first_name: string | null;
  department: string | null;
}

export interface AuthToken {
  access_token: string;
  token_type: string;
  expires_in: number;
  expires_at: number;
  user_id: string;
  username: string;
}

export interface LoginData {
  token: AuthToken;
  user: User;
}

export interface LoginResponse {
  success: boolean;
  message: string;
  data?: LoginData;
}

export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  message: string;
}

@Injectable({
  providedIn: 'root'
})
export class AuthService {
  private readonly TOKEN_KEY = 'auth_token';
  private readonly TOKEN_DATA_KEY = 'auth_token_data';
  private readonly USER_KEY = 'user_data';
  
  private currentUserSubject = new BehaviorSubject<User | null>(null);
  public currentUser$ = this.currentUserSubject.asObservable();
  
  private isAuthenticatedSubject = new BehaviorSubject<boolean>(false);
  public isAuthenticated$ = this.isAuthenticatedSubject.asObservable();

  private sessionExpiryWarningShown = false;

  constructor(
    private http: HttpClient,
    private configService: ConfigService,
    private debug: DebugService
  ) {
    this.initializeAuth();
  }

  private initializeAuth(): void {
    const token = this.getStoredToken();
    const tokenData = this.getStoredTokenData();
    const user = this.getStoredUser();
    
    if (token && tokenData && user && this.isTokenValid(tokenData)) {
      this.currentUserSubject.next(user);
      this.isAuthenticatedSubject.next(true);
    } else if (token || tokenData || user) {
      // Clear invalid/expired data
      this.logout();
    }
  }

  login(username: string, password: string): Observable<LoginResponse> {
    return this.http.post<LoginResponse>(`${this.configService.getApiUrl()}/auth/login`, {
      username,
      password
    }).pipe(
      map(response => {
        if (response.success && response.data) {
          this.storeAuthData(response.data);
          this.currentUserSubject.next(response.data.user);
          this.isAuthenticatedSubject.next(true);
        }
        return response;
      }),
      catchError(error => {
        console.error('Login error:', error);
        return throwError(() => ({
          success: false,
          message: 'Connection error. Please check if backend is running.'
        } as LoginResponse));
      })
    );
  }

  logout(): void {
    localStorage.removeItem(this.TOKEN_KEY);
    localStorage.removeItem(this.TOKEN_DATA_KEY);
    localStorage.removeItem(this.USER_KEY);
    this.currentUserSubject.next(null);
    this.isAuthenticatedSubject.next(false);
    this.sessionExpiryWarningShown = false;
    this.debug.info('AuthService', 'User logged out and session cleared');
  }

  testConnection(): Observable<boolean> {
    return this.http.get<ApiResponse<string>>(`${this.configService.getApiUrl()}/health`).pipe(
      map(response => response.success),
      catchError(() => {
        return throwError(() => false);
      })
    );
  }

  getStoredToken(): string | null {
    return localStorage.getItem(this.TOKEN_KEY);
  }

  getStoredUser(): User | null {
    const userJson = localStorage.getItem(this.USER_KEY);
    if (userJson) {
      try {
        return JSON.parse(userJson);
      } catch {
        return null;
      }
    }
    return null;
  }

  getCurrentUser(): User | null {
    return this.currentUserSubject.value;
  }

  isAuthenticated(): boolean {
    return this.isAuthenticatedSubject.value;
  }

  private storeAuthData(loginData: LoginData): void {
    localStorage.setItem(this.TOKEN_KEY, loginData.token.access_token);
    localStorage.setItem(this.TOKEN_DATA_KEY, JSON.stringify(loginData.token));
    localStorage.setItem(this.USER_KEY, JSON.stringify(loginData.user));
  }

  // For future authenticated requests
  getAuthHeaders(): { [key: string]: string } {
    const token = this.getStoredToken();
    const tokenData = this.getStoredTokenData();
    
    // Check if token is valid before using it
    if (token && tokenData && this.isTokenValid(tokenData)) {
      return {
        'Authorization': `Bearer ${token}`,
        'Content-Type': 'application/json'
      };
    } else {
      // Token is invalid or expired, logout
      if (token || tokenData) {
        console.warn('🚫 Token expired or invalid, logging out');
        this.logout();
      }
      return {
        'Content-Type': 'application/json'
      };
    }
  }

  getStoredTokenData(): AuthToken | null {
    const tokenDataJson = localStorage.getItem(this.TOKEN_DATA_KEY);
    if (tokenDataJson) {
      try {
        return JSON.parse(tokenDataJson);
      } catch {
        return null;
      }
    }
    return null;
  }

  isTokenValid(tokenData: AuthToken): boolean {
    const now = Math.floor(Date.now() / 1000); // Current time in seconds
    const expiresAt = tokenData.expires_at;
    const timeUntilExpiry = expiresAt - now;
    
    // Token is expired
    if (timeUntilExpiry <= 0) {
      console.warn('🕐 JWT token has expired');
      return false;
    }
    
    // Show warning if token expires in less than 15 minutes (900 seconds)
    if (timeUntilExpiry < 900 && !this.sessionExpiryWarningShown) {
      this.sessionExpiryWarningShown = true;
      const minutesLeft = Math.floor(timeUntilExpiry / 60);
      console.warn(`⚠️ Session will expire in ${minutesLeft} minutes`);
      // You could show a toast notification here
    }
    
    return true;
  }

  // Check if current session is authenticated and valid
  isValidSession(): boolean {
    const tokenData = this.getStoredTokenData();
    const user = this.getStoredUser();
    const token = this.getStoredToken();
    
    return !!(token && tokenData && user && this.isTokenValid(tokenData));
  }

  // Get remaining session time in seconds
  getRemainingSessionTime(): number {
    const tokenData = this.getStoredTokenData();
    if (!tokenData) return 0;
    
    const now = Math.floor(Date.now() / 1000);
    return Math.max(0, tokenData.expires_at - now);
  }

  // Check if user needs to reauthenticate
  checkAuthenticationStatus(): Observable<boolean> {
    if (!this.isValidSession()) {
      this.logout();
      return of(false);
    }

    // Verify with server
    return this.http.get<ApiResponse<boolean>>(`${this.configService.getApiUrl()}/auth/status`, {
      headers: this.getAuthHeaders()
    }).pipe(
      map(response => {
        if (!response.success || !response.data) {
          console.warn('🚫 Server rejected authentication status');
          this.logout();
          return false;
        }
        return true;
      }),
      catchError(() => {
        console.error('❌ Failed to verify authentication status with server');
        this.logout();
        return of(false);
      })
    );
  }
}