import { Routes } from '@angular/router';
import { LoginComponent } from './components/login/login.component';
import { PutawayComponent } from './components/putaway/putaway.component';
import { authGuard } from './guards/auth.guard';

// PUTAWAY ONLY: Routes for putaway app (no bulk-picking)
export const routes: Routes = [
  { path: '', redirectTo: '/login', pathMatch: 'full' },
  { path: 'login', component: LoginComponent },
  { path: 'putaway', component: PutawayComponent, canActivate: [authGuard] },
  { path: '**', redirectTo: '/login' }
];
