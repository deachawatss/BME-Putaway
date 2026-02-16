import { Component, OnInit, OnDestroy } from '@angular/core';
import { Router } from '@angular/router';
import { CommonModule } from '@angular/common';
import { AuthService, User } from '../../services/auth.service';

// shadcn/ui components - using only what we need for clean dashboard
import { CardComponent, CardContentComponent } from '../../../lib/ui/card.component';

@Component({
  selector: 'app-dashboard',
  standalone: true,
  imports: [
    CommonModule,
    CardComponent,
    CardContentComponent
  ],
  templateUrl: './dashboard.component.html'
})
export class DashboardComponent implements OnInit, OnDestroy {
  currentUser: User | null = null;

  // Only 2 menu options as requested
  menuOptions = [
    {
      id: 'putaway',
      title: 'Putaway',
      subtitle: 'Handle putaway operations',
      icon: '📦',
      route: '/putaway',
      color: 'bg-gradient-to-br from-[#059669] to-[#047857]', // Green gradient
      hoverColor: 'hover:from-[#059669] hover:to-[#065f46]'
    },
    {
      id: 'bulk-picking',
      title: 'Bulk Picking',
      subtitle: 'Manage bulk picking operations',
      icon: '🗃️',
      route: '/bulk-picking',
      color: 'bg-gradient-to-br from-[#3C2415] to-[#2D1B10]', // Enhanced brown gradient
      hoverColor: 'hover:from-[#3C2415] hover:to-[#1F120A]'
    }
  ];

  constructor(
    private authService: AuthService,
    private router: Router
  ) {}

  ngOnInit(): void {
    // Subscribe to current user
    this.authService.currentUser$.subscribe(user => {
      this.currentUser = user;
    });

    // Check authentication
    if (!this.authService.isAuthenticated()) {
      this.router.navigate(['/login']);
    }

  }

  ngOnDestroy(): void {
    // Component cleanup
  }

  onLogout(): void {
    this.authService.logout();
    this.router.navigate(['/login']);
  }

  navigateToMenu(route: string): void {
    this.router.navigate([route]);
  }

  getInitials(name: string): string {
    return name
      .split(' ')
      .map(n => n.charAt(0))
      .join('')
      .toUpperCase()
      .substring(0, 2);
  }

}