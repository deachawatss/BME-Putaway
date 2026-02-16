import { inject } from '@angular/core';
import { Router } from '@angular/router';
import { Observable } from 'rxjs';
import { map, catchError } from 'rxjs/operators';
import { AuthService } from '../services/auth.service';

export const authGuard = (): Observable<boolean> | boolean => {
  const authService = inject(AuthService);
  const router = inject(Router);

  // First check local token validity
  if (!authService.isValidSession()) {
    console.warn('🚫 AuthGuard: No valid local session');
    authService.logout();
    router.navigate(['/login']);
    return false;
  }

  // If local validation passes, verify with server
  return authService.checkAuthenticationStatus().pipe(
    map(isAuthenticated => {
      if (isAuthenticated) {
        console.debug('✅ AuthGuard: Authentication verified with server');
        return true;
      } else {
        console.warn('🚫 AuthGuard: Server authentication failed');
        router.navigate(['/login']);
        return false;
      }
    }),
    catchError(() => {
      console.error('❌ AuthGuard: Failed to verify authentication');
      authService.logout();
      router.navigate(['/login']);
      return [false];
    })
  );
};