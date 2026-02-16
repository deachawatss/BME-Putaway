import { Component, Input } from '@angular/core';
import { cn } from '../utils';

@Component({
  selector: 'app-skeleton',
  standalone: true,
  template: `<div 
    [class]="cn('bg-accent animate-pulse rounded-md', className)"
    [attr.data-slot]="'skeleton'">
  </div>`,
})
export class SkeletonComponent {
  @Input() className = '';
  
  protected cn = cn;
}