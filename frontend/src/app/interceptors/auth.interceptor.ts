import { Injectable } from '@angular/core';
import { HttpInterceptor, HttpRequest, HttpHandler, HttpEvent, HttpErrorResponse } from '@angular/common/http';
import { Observable, throwError } from 'rxjs';
import { catchError } from 'rxjs/operators';
import { Router } from '@angular/router';
import { AuthService } from '../services/auth.service';

@Injectable()
export class AuthInterceptor implements HttpInterceptor {
  constructor(
    private authService: AuthService,
    private router: Router
  ) {}

  intercept(req: HttpRequest<any>, next: HttpHandler): Observable<HttpEvent<any>> {
    // Skip interceptor for login and health check endpoints
    const isAuthEndpoint = req.url.includes('/auth/login') || req.url.includes('/health');
    
    if (!isAuthEndpoint) {
      // Check if we have a valid session before making the request
      if (!this.authService.isValidSession()) {
        console.warn('🚫 No valid session for API request, redirecting to login');
        this.authService.logout();
        this.router.navigate(['/login']);
        return throwError(() => new Error('No valid session'));
      }

      // Clone the request and add authorization header
      const authHeaders = this.authService.getAuthHeaders();
      if (authHeaders['Authorization']) {
        req = req.clone({
          setHeaders: authHeaders
        });
        console.debug('🔐 Added Authorization header to request:', req.url);
      }
    }

    return next.handle(req).pipe(
      catchError((error: HttpErrorResponse) => {
        // Handle 401 Unauthorized responses
        if (error.status === 401 && !isAuthEndpoint) {
          console.warn('🚫 Received 401 Unauthorized, token may be expired');
          this.authService.logout();
          this.router.navigate(['/login']);
          return throwError(() => error);
        }

        // Handle 403 Forbidden responses
        if (error.status === 403) {
          console.warn('🚫 Access forbidden (403)');
          // Could show a "insufficient permissions" message
        }

        // Handle network errors
        if (error.status === 0) {
          console.error('🌐 Network error - backend server may be down');
        }

        return throwError(() => error);
      })
    );
  }
}