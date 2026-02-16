import { Component, OnInit } from '@angular/core';
import { FormBuilder, FormGroup, Validators, ReactiveFormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { CommonModule } from '@angular/common';
import { AuthService } from '../../services/auth.service';

// No shadcn/ui imports needed - using native HTML elements

@Component({
  selector: 'app-login',
  standalone: true,
  imports: [
    CommonModule,
    ReactiveFormsModule
  ],
  templateUrl: './login.component.html'
})
export class LoginComponent implements OnInit {
  loginForm: FormGroup;
  isLoading = false;
  connectionStatus: 'unknown' | 'connected' | 'disconnected' = 'unknown';

  constructor(
    private fb: FormBuilder,
    private authService: AuthService,
    private router: Router
  ) {
    this.loginForm = this.fb.group({
      username: ['', [Validators.required, Validators.minLength(2)]],
      password: ['', [Validators.required, Validators.minLength(1)]]
    });
  }

  ngOnInit(): void {
    // Check if already authenticated
    if (this.authService.isAuthenticated()) {
      this.router.navigate(['/dashboard']);
      return;
    }

    // Test backend connection
    this.testConnection();
  }

  testConnection(): void {
    this.authService.testConnection().subscribe({
      next: (isConnected) => {
        this.connectionStatus = isConnected ? 'connected' : 'disconnected';
      },
      error: () => {
        this.connectionStatus = 'disconnected';
      }
    });
  }

  onSubmit(): void {
    if (this.loginForm.valid && !this.isLoading) {
      this.isLoading = true;
      // Disable form while loading
      this.loginForm.disable();
      const { username, password } = this.loginForm.value;

      this.authService.login(username, password).subscribe({
        next: (response) => {
          this.isLoading = false;
          this.loginForm.enable();
          if (response.success) {
            // Success feedback handled by navigation
            this.router.navigate(['/dashboard']);
          } else {
            // Error will be shown inline in the form
            console.error('Login failed:', response.message);
          }
        },
        error: (error) => {
          this.isLoading = false;
          this.loginForm.enable();
          console.error('Login error:', error.message || 'Connection error');
        }
      });
    } else {
      // Mark all fields as touched to show validation errors
      Object.keys(this.loginForm.controls).forEach(key => {
        this.loginForm.get(key)?.markAsTouched();
      });
    }
  }

  getConnectionStatusText(): string {
    switch (this.connectionStatus) {
      case 'connected':
        return 'Backend Connected';
      case 'disconnected':
        return 'Backend Disconnected';
      default:
        return 'Checking Connection...';
    }
  }

  getConnectionStatusIcon(): string {
    switch (this.connectionStatus) {
      case 'connected':
        return 'check_circle';
      case 'disconnected':
        return 'error';
      default:
        return 'hourglass_empty';
    }
  }

  getConnectionStatusClass(): string {
    switch (this.connectionStatus) {
      case 'connected':
        return 'connected';
      case 'disconnected':
        return 'disconnected';
      default:
        return 'connecting';
    }
  }

  getConnectionStatusIndicatorClass(): string {
    switch (this.connectionStatus) {
      case 'connected':
        return 'nwfth-status-indicator--connected';
      case 'disconnected':
        return 'nwfth-status-indicator--disconnected';
      default:
        return 'nwfth-status-indicator--connecting';
    }
  }

  getConnectionStatusAriaLabel(): string {
    switch (this.connectionStatus) {
      case 'connected':
        return 'System connected successfully';
      case 'disconnected':
        return 'System connection failed';
      default:
        return 'Checking system connection';
    }
  }

  getConnectionStatusVariant(): 'default' | 'secondary' | 'destructive' | 'outline' {
    switch (this.connectionStatus) {
      case 'connected':
        return 'default';
      case 'disconnected':
        return 'destructive';
      default:
        return 'secondary';
    }
  }

  getEnhancedConnectionStatusClass(): string {
    switch (this.connectionStatus) {
      case 'connected':
        return 'nwfth-status-connected';
      case 'disconnected':
        return 'nwfth-status-disconnected';
      default:
        return 'nwfth-status-connecting';
    }
  }

  getStatusIndicatorClass(): string {
    switch (this.connectionStatus) {
      case 'connected':
        return 'nwfth-indicator-connected';
      case 'disconnected':
        return 'nwfth-indicator-disconnected';
      default:
        return 'nwfth-indicator-connecting';
    }
  }

  getStatusPingClass(): string {
    return 'nwfth-ping-connecting';
  }
}